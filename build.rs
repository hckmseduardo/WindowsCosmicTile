#[cfg(windows)]
fn main() {
    println!("cargo:rerun-if-changed=assets/brand/CosmicTile.ico");
    winresource::WindowsResource::new()
        .set_icon("assets/brand/CosmicTile.ico")
        .set("ProductName", "CosmicTile")
        .set("FileDescription", "CosmicTile window manager")
        .set("OriginalFilename", "WindowsCosmicTile.exe")
        .compile()
        .expect("Failed to embed the shared CosmicTile application icon");
}

// Cross-target cargo check on non-Windows hosts validates code only. The native
// Windows CI builds compile and verify the actual icon resource on both CPUs.
#[cfg(not(windows))]
fn main() {
    println!("cargo:rerun-if-changed=assets/brand/CosmicTile.ico");
}
