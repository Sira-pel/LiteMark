//! One running LiteMark per user: launching the app again with a file hands
//! that file to the running window instead of opening a second window.
//!
//! Over a socket in the user's runtime directory on Unix, and over a named
//! pipe on Windows.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

pub enum Claim {
    /// Another LiteMark took the file; this process should exit.
    HandedOff,
    /// This process is the main one. Files sent by later launches arrive here.
    Primary(Option<Receiver<PathBuf>>),
}

#[cfg(unix)]
pub fn claim(file: Option<&std::path::Path>) -> Claim {
    let Some(dir) = dirs::runtime_dir().or_else(dirs::cache_dir) else {
        return Claim::Primary(None);
    };
    // The socket lives in a directory only this user can enter, so it is never
    // reachable by others, even in the moment between `bind` and `chmod`.
    let private = dir.join("litemark");
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let made = std::fs::DirBuilder::new().mode(0o700).create(&private);
        if made.is_err() && !private.is_dir() {
            return Claim::Primary(None);
        }
        let _ = std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700));
    }
    claim_at(&private.join("litemark.sock"), file)
}

#[cfg(windows)]
pub fn claim(file: Option<&std::path::Path>) -> Claim {
    // Pipe names are machine-wide, so keep each user's, and each sign-in
    // session's (Remote Desktop next to the local screen), apart.
    let user: String = std::env::var("USERNAME")
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    claim_at(&format!(r"\\.\pipe\litemark-{user}-{}", session_id()), file)
}

#[cfg(windows)]
fn session_id() -> u32 {
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    let mut session = 0;
    unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) };
    session
}

/// The user a process runs as, as a `TOKEN_USER` buffer (8-byte aligned).
#[cfg(windows)]
fn token_user(process: windows_sys::Win32::Foundation::HANDLE) -> Option<Vec<u64>> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY};
    use windows_sys::Win32::System::Threading::OpenProcessToken;

    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    let mut len = 0;
    unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut len) };
    let mut buf = vec![0u64; (len as usize).div_ceil(8).max(1)];
    let ok = len != 0
        && unsafe { GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), len, &mut len) }
            != 0;
    unsafe { CloseHandle(token) };
    ok.then_some(buf)
}

/// Whether the process serving this pipe runs as the same user as we do. The
/// pipe name is machine-wide, so another user could have created it first and
/// would otherwise be handed our file paths.
#[cfg(windows)]
fn served_by_us(pipe: &std::fs::File) -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{EqualSid, TOKEN_USER};
    use windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle() as _, &mut pid) } == 0 {
        return false;
    }
    let server = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if server.is_null() {
        return false;
    }
    let theirs = token_user(server);
    unsafe { CloseHandle(server) };
    let ours = token_user(unsafe { GetCurrentProcess() });
    match (theirs, ours) {
        (Some(a), Some(b)) => unsafe {
            let a = (*(a.as_ptr() as *const TOKEN_USER)).User.Sid;
            let b = (*(b.as_ptr() as *const TOKEN_USER)).User.Sid;
            EqualSid(a, b) != 0
        },
        _ => false,
    }
}

#[cfg(not(any(unix, windows)))]
pub fn claim(_file: Option<&std::path::Path>) -> Claim {
    Claim::Primary(None)
}

/// `canonicalize` gives `\\?\C:\...` on Windows; drop the prefix so the path
/// matches the ones tabs already hold.
#[cfg(windows)]
fn plain_path(path: &std::path::Path) -> PathBuf {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    match path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path,
    }
}

#[cfg(windows)]
fn connect_pipe(pipe: &str) -> Option<std::fs::File> {
    const ERROR_PIPE_BUSY: i32 = 231;
    for _ in 0..20 {
        match std::fs::OpenOptions::new().write(true).open(pipe) {
            Ok(f) => return Some(f),
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
    None
}

/// One server end of the pipe. The first one claims the name, so a second
/// LiteMark can't also become the main one. Returned as an integer so it can
/// move between threads.
#[cfg(windows)]
fn create_pipe_instance(pipe: &str, first: bool) -> Option<usize> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND,
    };
    use windows_sys::Win32::System::Pipes::{
        CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };

    let wide: Vec<u16> = std::ffi::OsStr::new(pipe)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut open_mode = PIPE_ACCESS_INBOUND;
    if first {
        open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    // Default security: only this user (and admins) may write to the pipe.
    let handle = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            open_mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            0,
            8192,
            0,
            std::ptr::null(),
        )
    };
    (handle != INVALID_HANDLE_VALUE).then_some(handle as usize)
}

#[cfg(windows)]
fn claim_at(pipe: &str, file: Option<&std::path::Path>) -> Claim {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_PIPE_CONNECTED};
    use windows_sys::Win32::System::Pipes::ConnectNamedPipe;
    use windows_sys::Win32::System::IO::CancelIoEx;

    if let Some(mut stream) = connect_pipe(pipe) {
        let Some(file) = file else {
            // Another LiteMark is already running: open a plain window.
            return Claim::Primary(None);
        };
        if !served_by_us(&stream) {
            return Claim::Primary(None);
        }
        let mut message = plain_path(file).to_string_lossy().into_owned().into_bytes();
        message.push(b'\n');
        return if stream.write_all(&message).is_ok() {
            Claim::HandedOff
        } else {
            // It answered but took nothing.
            Claim::Primary(None)
        };
    }
    let Some(first) = create_pipe_instance(pipe, true) else {
        return Claim::Primary(None);
    };
    let pipe = pipe.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut current = first;
        loop {
            let connected = unsafe { ConnectNamedPipe(current as _, std::ptr::null_mut()) };
            if connected == 0 && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED {
                unsafe { CloseHandle(current as _) };
                match create_pipe_instance(&pipe, false) {
                    Some(next) => current = next,
                    None => return,
                }
                continue;
            }
            // Open the next end before serving this one, so the name is never
            // free for another process to claim.
            let next = create_pipe_instance(&pipe, false);
            let stream = std::sync::Arc::new(unsafe {
                std::fs::File::from_raw_handle(current as RawHandle)
            });
            let tx = tx.clone();
            // Its own thread: a client that connects and says nothing must
            // not block the rest.
            std::thread::spawn(move || {
                let (done_tx, done_rx) = std::sync::mpsc::channel();
                let reader = stream.clone();
                std::thread::spawn(move || {
                    let mut line = Vec::new();
                    // A path is short; cap the read so a stray client can't
                    // grow it.
                    let read = BufReader::new((&*reader).take(8192)).read_until(b'\n', &mut line);
                    let _ = done_tx.send(read.map(|_| line));
                });
                let mut line = match done_rx.recv_timeout(std::time::Duration::from_secs(2)) {
                    Ok(Ok(line)) => line,
                    Ok(Err(_)) => return,
                    Err(_) => {
                        // Silent client: abort the blocked read so its thread
                        // ends and the pipe closes. `stream` is still held
                        // here, so the handle is valid.
                        unsafe { CancelIoEx(stream.as_raw_handle() as _, std::ptr::null()) };
                        return;
                    }
                };
                while matches!(line.last(), Some(b'\n' | b'\r')) {
                    line.pop();
                }
                if !line.is_empty() {
                    let _ = tx.send(PathBuf::from(String::from_utf8_lossy(&line).into_owned()));
                }
            });
            match next {
                Some(next) => current = next,
                None => return,
            }
        }
    });
    Claim::Primary(Some(rx))
}

#[cfg(unix)]
fn claim_at(sock: &std::path::Path, file: Option<&std::path::Path>) -> Claim {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};

    if let Ok(mut stream) = UnixStream::connect(sock) {
        let Some(file) = file else {
            // Another LiteMark is already running: open a plain window and
            // leave its socket alone so it stays reachable.
            return Claim::Primary(None);
        };
        let path = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
        let sent = stream.write_all(path.as_os_str().as_encoded_bytes());
        if sent.is_ok() && stream.write_all(b"\n").is_ok() {
            return Claim::HandedOff;
        }
        // It answered but took nothing: don't tear its socket down.
        return Claim::Primary(None);
    }
    // Nobody is listening (or the socket is left over from a crash).
    let _ = std::fs::remove_file(sock);
    let Ok(listener) = UnixListener::bind(sock) else {
        return Claim::Primary(None);
    };
    let _ = std::fs::set_permissions(sock, std::fs::Permissions::from_mode(0o600));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            // A client that connects and says nothing must not block the rest.
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
            let mut line = Vec::new();
            // A path is short; cap the read so a stray client can't grow it.
            if BufReader::new(stream.take(8192))
                .read_until(b'\n', &mut line)
                .is_err()
            {
                continue;
            }
            while line.last() == Some(&b'\n') {
                line.pop();
            }
            if line.is_empty() {
                continue;
            }
            use std::os::unix::ffi::OsStringExt;
            let path = PathBuf::from(std::ffi::OsString::from_vec(line));
            if tx.send(path).is_err() {
                break;
            }
        }
    });
    Claim::Primary(Some(rx))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_second_launch_hands_its_file_to_the_first() {
        let dir = std::env::temp_dir().join(format!("litemark-si-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("litemark.sock");
        let file = dir.join("a.md");
        std::fs::write(&file, "x").unwrap();

        let Claim::Primary(Some(rx)) = claim_at(&sock, None) else {
            panic!("first launch should be the main one");
        };
        assert!(matches!(claim_at(&sock, Some(&file)), Claim::HandedOff));
        let got = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        assert_eq!(got, file.canonicalize().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn a_second_launch_hands_its_file_to_the_first() {
        let pipe = format!(r"\\.\pipe\litemark-test-{}", std::process::id());
        let dir = std::env::temp_dir().join(format!("litemark-si-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.md");
        std::fs::write(&file, "x").unwrap();

        let Claim::Primary(Some(rx)) = claim_at(&pipe, None) else {
            panic!("first launch should be the main one");
        };
        // A launch with no file leaves the pipe alone.
        assert!(matches!(claim_at(&pipe, None), Claim::Primary(None)));
        // The name is taken: nobody else can claim it as the first instance.
        assert!(create_pipe_instance(&pipe, true).is_none());
        for _ in 0..2 {
            assert!(matches!(claim_at(&pipe, Some(&file)), Claim::HandedOff));
            let got = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
            assert_eq!(got, plain_path(&file));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(all(test, windows))]
mod windows_silent_client_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_silent_client_is_dropped_and_does_not_block_others() {
        let pipe = format!(r"\\.\pipe\litemark-test-silent-{}", std::process::id());
        let Claim::Primary(Some(rx)) = claim_at(&pipe, None) else {
            panic!("first launch should be the main one");
        };
        let mut silent = connect_pipe(&pipe).unwrap();
        // Another launch still gets through while the silent one is connected.
        let file = std::env::temp_dir();
        assert!(matches!(claim_at(&pipe, Some(&file)), Claim::HandedOff));
        rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        // After the timeout the server closes its end, so writing fails.
        std::thread::sleep(std::time::Duration::from_millis(3000));
        assert!(silent.write_all(b"late\n").is_err());
    }
}
