//! Windows only: embed version info (VERSIONINFO) into `strix-cli.exe`. Code signing checks the
//! product name and version of every signed file (docs/code-signing.md), and Explorer shows them.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, unless
//! `STRIX_REQUIRE_WINRES=1` turns it into an error (for release builds).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=STRIX_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set("ProductName", "Strix")
        .set("FileDescription", "Strix command line")
        .set("CompanyName", "The Strix contributors")
        .set("LegalCopyright", "Copyright (c) the Strix contributors and the WordCraft authors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "strix-cli.exe")
        .set("InternalName", "strix-cli");
    if let Err(e) = res.compile() {
        if std::env::var_os("STRIX_REQUIRE_WINRES").is_some() {
            eprintln!("embedding Windows resources failed: {e}");
            std::process::exit(1);
        }
        println!("cargo:warning=strix-cli.exe built without version resources: {e}");
    }
}
