use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // gpui 0.2.2 enables the `x11` feature of the `xkbcommon` crate, and that
    // crate links with `-lxkbcommon-x11`. The linker therefore needs the
    // unversioned SONAME `libxkbcommon-x11.so`.
    //
    // Ubuntu's runtime package `libxkbcommon-x11-0` ships only
    // `libxkbcommon-x11.so.0`. The unversioned name is installed by
    // `libxkbcommon-x11-dev`. On a machine that has the runtime library but not
    // the -dev package, rustc cannot resolve `-lxkbcommon-x11` unless we expose
    // the versioned library under the name the linker asks for.
    //
    // When the -dev symlink is already present this block does nothing. It is
    // still required for gpui 0.2.2; the dependency has not dropped xkbcommon.
    let lib_paths = [
        "/usr/lib/x86_64-linux-gnu/libxkbcommon-x11.so",
        "/usr/lib/libxkbcommon-x11.so",
        "/usr/lib64/libxkbcommon-x11.so",
    ];

    let found = lib_paths.iter().any(|p| std::path::Path::new(p).exists());
    if !found {
        let candidate_so0 = [
            "/usr/lib/x86_64-linux-gnu/libxkbcommon-x11.so.0",
            "/usr/lib/libxkbcommon-x11.so.0",
            "/usr/lib64/libxkbcommon-x11.so.0",
        ];

        for so0 in &candidate_so0 {
            if std::path::Path::new(so0).exists() {
                let target = out_dir.join("libxkbcommon-x11.so");
                if !target.exists() {
                    let _ = std::os::unix::fs::symlink(so0, &target);
                }
                println!("cargo:rustc-link-search=native={}", out_dir.display());
                break;
            }
        }
    }
}
