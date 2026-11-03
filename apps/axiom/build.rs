//! Windows only: embed the app icon and version info (VERSIONINFO) into `axiom.exe`.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `AXIOM_REQUIRE_WINRES=1` (set by
//! the release workflow) turns it into an error.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/axiom.ico");
    println!("cargo:rerun-if-env-changed=AXIOM_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/axiom.ico")
        .set("ProductName", "Axiom")
        .set("FileDescription", "Axiom: find out how any software works")
        .set("LegalCopyright", "Copyright (c) the ArtCraft Team and the Axiom contributors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "axiom.exe")
        .set("InternalName", "axiom");
    if let Err(e) = res.compile() {
        if std::env::var_os("AXIOM_REQUIRE_WINRES").is_some() {
            println!("cargo::error=embedding Windows resources failed: {e}");
            return;
        }
        println!("cargo:warning=axiom.exe built without icon/version resources: {e}");
    }
}
