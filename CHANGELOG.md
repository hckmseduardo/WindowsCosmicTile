# Changelog

## 0.1.0

### Added
- Native Rust + Win32 notification-area application with no web runtime.
- x64 and ARM64 Windows builds with independent CI jobs and PE architecture checks.
- Per-monitor tiling activation, automatic layout splits and larger-region placement.
- Directional focus, window swaps, split rotation, floating and monitor transfer.
- Temporary resize-mode arrow shortcuts, Escape/Enter exit, and direct width/height adjustments.
- Click-through active-window hint with cyan, violet and orange accents.
- Persistent gap, hint and language preferences.
- English default with French and Portuguese tray translations.
- Original window placement restoration on normal quit or monitor disable.
- DPI-aware work-area geometry and DWM visible-border compensation.
- Structural exclusion of save/open dialogs, owned modal windows, utility and fixed-size windows.
- Unit tests, dual-architecture build/release workflow and portable ZIP packaging.

### Known limitations
- Initial preview: interactive application compatibility and multi-monitor/DPI scenarios need live Windows validation.
- Minimum-size heuristics do not guarantee non-overlap for all applications.
- No tabbed stacks, drag previews, startup registration or per-virtual-desktop layout persistence.
- Unsigned executable; no Authenticode certificate is configured.
