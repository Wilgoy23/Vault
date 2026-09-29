fn main() {
    // On iOS the crate calls AutoFill functions exported by Swift in the app
    // target (see src/autofill.rs). They resolve when the static library is
    // linked into the app, but the cdylib that Cargo also builds has no Swift
    // to link against, so let those symbols stay unresolved there.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-undefined,dynamic_lookup");
    }
    tauri_build::build()
}
