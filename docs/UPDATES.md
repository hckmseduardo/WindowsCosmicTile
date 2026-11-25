# Release updates

From v0.2.3, CosmicTile can download and install an update in place after you approve it. It closes normally, replaces the executable at its current path, and reopens. Saved preferences and shortcuts continue to work. On restart, tiling starts off as usual.

If you have v0.2.2 or earlier, quit CosmicTile and replace your current executable with the matching `.exe` from the newest preview on GitHub Releases. Keep the same file name to preserve any shortcuts. The ZIP stays available for first-time installation.

## Checks and consent

- Startup and daily checks retrieve public release metadata from `https://api.github.com/repos/hckmseduardo/WindowsCosmicTile/releases?per_page=100`. No token, account or credentials are used. This request exposes normal network information, including your IP address, to GitHub.
- Automatic checks default to on. The tray checkbox, last-check timestamp and declined version persist in `%APPDATA%/WindowsCosmicTile/settings.conf` with other preferences. Checks are attempted no more than once per 24 hours, including after failure. The tray command can check immediately.
- Drafts, invalid versions and incompatible assets are ignored. Stable and preview releases are eligible. The highest Semantic Version above the running version with a valid asset is selected.
- A native Yes/No dialog displays the version and release notes. **No is the default.** Only Yes authorizes downloading and installing the update.
- Declining suppresses further automatic prompts for that version. A manual check allows retry. A newer version can prompt again.
- Automatic no-update and failure results stay quiet. Manual checks show a result. Only one check, confirmation or download may be active at a time.

## Asset and transport rules

The updater selects `WindowsCosmicTile-vVERSION-windows-x64.exe` or `WindowsCosmicTile-vVERSION-windows-arm64.exe` to match the running build. The GitHub asset must be uploaded, advertise a positive size no greater than 512 MiB, and include a `sha256:` digest with exactly 64 hexadecimal digits.

The browser download URL must exactly match the expected HTTPS GitHub owner, repository, release tag and filename. HTTPS redirects are bounded to five hops and restricted to GitHub and its release-asset storage hosts. Requests use 10-second connection and 180-second total timeouts. Release metadata is limited to 8 MiB. Windows uses the operating system's TLS certificate validation through Schannel; portable macOS tests use rustls.

The app streams the executable into a temporary folder beside the installed copy, checks its exact byte count and SHA-256, and confirms the Windows executable's CPU architecture. Run the app as a regular user with write access to its folder.

A temporary copy of the original app waits for normal shutdown, replaces the file, and launches the update from the original path. The new process signals when it initializes. If it cannot launch or initialize, the helper restores and reopens the previous version. The previous executable stays available in the temporary folder if Windows also prevents recovery.

## Integrity and signing

The GitHub asset digest detects corruption or a mismatch with the bytes advertised by GitHub. It is **not publisher code signing**, an independent authenticity signature, or protection against a compromised GitHub release/account. These preview executables are not Authenticode signed, and Windows SmartScreen may warn. This updater does not bypass operating-system security prompts.

## Validation

Portable tests cover release selection, consent, verification, replacement and rollback. CI builds both Windows architectures and tests the native helper's restart and recovery paths.
