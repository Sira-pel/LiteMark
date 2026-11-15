//! One running LiteMark per user: launching the app again with a file hands
//! that file to the running window instead of opening a second window.
//!
//! Unix only, over a socket in the user's runtime directory. Elsewhere the
//! app always starts its own window.

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
    claim_at(&dir.join("litemark.sock"), file)
}

#[cfg(not(unix))]
pub fn claim(_file: Option<&std::path::Path>) -> Claim {
    Claim::Primary(None)
}

#[cfg(unix)]
fn claim_at(sock: &std::path::Path, file: Option<&std::path::Path>) -> Claim {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};

    if let Some(file) = file {
        if let Ok(mut stream) = UnixStream::connect(sock) {
            let path = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
            if stream.write_all(path.as_os_str().as_encoded_bytes()).is_ok()
                && stream.write_all(b"\n").is_ok()
            {
                return Claim::HandedOff;
            }
        }
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
            let mut line = Vec::new();
            // A path is short; cap the read so a stray client can't grow it.
            if BufReader::new(stream.take(8192)).read_until(b'\n', &mut line).is_err() {
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
