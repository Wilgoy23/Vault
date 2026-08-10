// Prevents a console window appearing on Windows in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// All app logic lives in lib.rs so it can be shared with the mobile
// entry point (iOS/Android build the library, not this binary).
fn main() {
    vault_lib::run()
}
