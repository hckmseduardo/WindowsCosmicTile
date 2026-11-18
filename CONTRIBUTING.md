# Contributing

Keep code, comments, README and changelog in English. UI translations belong in src/i18n.rs, using canonical English keys.

On Windows, install the stable Rust MSVC toolchain and Visual Studio Build Tools with the C++ workload and Windows SDK. Run:

```powershell
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets -- -D warnings
cargo build --release
```

The pure layout tests can run on macOS/Linux too. Win32 adapters must be checked on Windows or with a Windows cross-target. Keep unsafe operations inside src/platform and keep the layout engine independent of Win32 handles.

For bugs, include Windows version, CPU architecture, display arrangement/scaling, affected application, steps, expected result and actual result. Do not include private window content. Restore positions by quitting through the tray before replacing the executable.
