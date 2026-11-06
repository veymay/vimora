//! Embeds the Vimora icon into the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/brand/vimora.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/brand/vimora.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
}
