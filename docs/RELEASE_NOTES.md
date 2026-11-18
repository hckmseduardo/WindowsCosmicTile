## WindowsCosmiTile v0.1.0 preview

Native Rust + Win32 builds for **Windows x64** and **Windows ARM64**. Choose the ZIP matching your CPU; the ARM64 asset is a native ARM64 build, not an x64 executable with a different filename.

### Features
- Independent tiling per monitor; new displays and startup default to floating.
- Automatic horizontal/vertical layout, focus, swaps, rotation and monitor transfer.
- Keyboard resize mode with arrow keys and Escape/Enter exit.
- Click-through active-window outline with three accent colors.
- Native notification-area menu, gap controls and keyboard guide.
- English by default, with French and Portuguese tray UI options.
- Save/Open dialogs, owned modals, tool windows and fixed-size auxiliary windows excluded from tiling.
- Best-effort window restoration on disable or normal quit.

### Install
Extract the matching ZIP and launch WindowsCosmiTile.exe. Use the tray menu to enable your displays. No web runtime or installer is needed. The executable is **not Authenticode signed**; Windows SmartScreen can warn on first launch.

### Verification and limitations
Both architecture packages are produced on Windows runners after formatting, unit tests, native-code lint and release compilation. The workflow also checks the PE machine type of each executable. Hosted builds and unit tests do not establish interactive multi-monitor behavior on your own computer; app-specific sizing, DPI changes and ARM64 hardware still require live validation.

The layout engine uses minimum-size heuristics, not a full application-constraint solver. No tabbed stacks, drag previews, persistent per-virtual-desktop layouts or auto-start installation yet. Native windows may refuse movement across privilege boundaries. Forced process termination cannot guarantee position restoration.

Each ZIP includes README, changelog and quick-start instructions. A SHA-256 file accompanies each asset.
