# WindowsCosmicTile v0.1

Approved direction: native Rust + Win32, no web runtime. Windows 10/11 x86-64 and ARM64 builds. The app starts with tiling off and lives in the notification area.

Pure Rust layout and monitor state are separated from unsafe Win32 adapters. Each enabled monitor has a binary layout tree. Global hotkeys provide focus, swap, resize, rotate, float and monitor transfer. A nonactivating click-through outline highlights the focused managed window. Work areas come from MONITORINFO.rcWork under per-monitor DPI awareness, including negative coordinates and taskbar reservation.

Use EnumWindows to discover standard visible unowned top-level windows, exclude cloaked/fullscreen/minimized windows, and reconcile membership without continually rewriting stable frames. Keep original window placements for restoration on disable/normal exit. New monitors start disabled. Native tray menus expose per-monitor switches and keyboard help.

Verification: host unit tests for tree geometry, insertion, resize and independent monitor state; cross-target type checks for x86_64-pc-windows-msvc and aarch64-pc-windows-msvc; Windows CI builds/tests and packages both executables. A successful compile is not interactive desktop or ARM64 hardware validation.
