#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(any(windows, test))]
mod installer;
#[cfg(windows)]
mod platform;
fn main() {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    #[cfg(windows)]
    if let Some(status) = platform::installer::dispatch(&arguments) {
        std::process::exit(status);
    }
    if arguments.iter().any(|argument| argument == "--version") {
        println!("CosmicTile {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    #[cfg(windows)]
    platform::run(platform::installer::restart_stage(&arguments));
    #[cfg(not(windows))]
    eprintln!("WindowsCosmicTile runs on Windows 10/11. Use cargo test to test the portable layout engine.");
}
