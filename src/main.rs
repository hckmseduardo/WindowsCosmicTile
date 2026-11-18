#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod platform;
fn main() {
    if std::env::args().any(|argument| argument == "--version") {
        println!("WindowsCosmicTile {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    #[cfg(windows)]
    platform::run();
    #[cfg(not(windows))]
    eprintln!("WindowsCosmicTile runs on Windows 10/11. Use cargo test to test the portable layout engine.");
}
