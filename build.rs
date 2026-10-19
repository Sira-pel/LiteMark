use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Check if libxkbcommon-x11.so is available in standard search paths
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
