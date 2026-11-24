use flate2::write::DeflateEncoder;
use flate2::Compression;
use std::io::Write;
use std::path::Path;

const FONT_FILES: &[&str] = &[
    "Ubuntu-R.ttf",
    "Ubuntu-RI.ttf",
    "Ubuntu-M.ttf",
    "Ubuntu-B.ttf",
    "Ubuntu-BI.ttf",
    "UbuntuMono-R.ttf",
    "UbuntuMono-RI.ttf",
    "UbuntuMono-B.ttf",
    "UbuntuMono-BI.ttf",
];

fn main() {
    let out_dir = std::env::var_os("OUT_DIR").expect("OUT_DIR");
    let out_dir = Path::new(&out_dir);
    for name in FONT_FILES {
        let src = Path::new("assets/fonts").join(name);
        println!("cargo:rerun-if-changed=assets/fonts/{name}");
        let raw = std::fs::read(&src).unwrap_or_else(|e| panic!("read {}: {e}", src.display()));
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
        encoder
            .write_all(&raw)
            .unwrap_or_else(|e| panic!("compress {name}: {e}"));
        let compressed = encoder
            .finish()
            .unwrap_or_else(|e| panic!("finish {name}: {e}"));
        let dst = out_dir.join(format!("{name}.deflate"));
        std::fs::write(&dst, compressed).unwrap_or_else(|e| panic!("write {}: {e}", dst.display()));
    }

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/windows/litemark.ico");
        res.set("ProductName", "LiteMark");
        res.set("FileDescription", "LiteMark");
        if let Err(err) = res.compile() {
            panic!("could not embed the Windows icon: {err}");
        }
    }
}
