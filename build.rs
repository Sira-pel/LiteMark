fn main() {
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
