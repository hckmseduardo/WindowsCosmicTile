# Release updates

CosmicTile v0.2.0 introduces **download-only updates**. It never replaces or launches downloaded executables, extracts archives, requests elevation, or installs anything. Quit CosmicTile, extract the verified ZIP, and replace your old copy manually.

## Checks and consent

- Startup and daily checks retrieve public release metadata from `https://api.github.com/repos/hckmseduardo/WindowsCosmicTile/releases?per_page=100`. No token, account or credentials are used. This request exposes normal network information, including your IP address, to GitHub.
- Automatic checks default to on. The tray checkbox, last-check timestamp and declined version persist in `%APPDATA%/WindowsCosmicTile/settings.conf` with other preferences. Checks are attempted no more than once per 24 hours, including after failure. The tray command can check immediately.
- Drafts, invalid versions and incompatible assets are ignored. Stable and preview releases are eligible. The highest Semantic Version above the running version with a valid asset is selected.
- A native Yes/No dialog displays the version and release notes. **No is the default.** Only Yes authorizes an asset HTTP request. Checks and declines never fetch an archive.
- Declining suppresses further automatic prompts for that version. A manual check allows retry. A newer version can prompt again.
- Automatic no-update and failure results stay quiet. Manual checks show a result. Only one check, confirmation or download may be active at a time.

## Asset and transport rules

Only `WindowsCosmicTile-vVERSION-windows-x64.zip` or `WindowsCosmicTile-vVERSION-windows-arm64.zip`, matching the running build's native CPU, is accepted. The GitHub asset must be uploaded, advertise a positive size no greater than 512 MiB, and include a `sha256:` digest with exactly 64 hexadecimal digits. Missing or invalid integrity metadata makes the release ineligible.

The browser download URL must exactly match the expected HTTPS GitHub owner, repository, release tag and filename. HTTPS redirects are bounded to five hops and restricted to GitHub and its release-asset storage hosts. Requests use 10-second connection and 180-second total timeouts. Release metadata is limited to 8 MiB. Windows uses the operating system's TLS certificate validation through Schannel; portable macOS tests use rustls.

Network operations and file verification run on a background worker. The Win32 timer drains completion events on the owning UI thread. A unique `.part` file is streamed under the Windows known Downloads folder, inside `CosmicTile Updates`. Stream length and SHA-256 must match before atomic, non-overwriting publication of the ZIP. Existing matching archives can be reused; a mismatching file is never overwritten. Failures remove the current partial file. A forced process kill can leave a partial file, which is never treated as an installable archive. Completion opens the folder, not the ZIP or executable.

## Integrity and signing

The GitHub asset digest detects corruption or a mismatch with the bytes advertised by GitHub. It is **not publisher code signing**, an independent authenticity signature, or protection against a compromised GitHub release/account. These preview executables are not Authenticode signed, and Windows SmartScreen may warn. This updater does not bypass operating-system security prompts.

## Validation

Portable tests cover release ordering, preview inclusion, strict architecture/URL/digest/size/state selection, single-flight/decline behavior, daily scheduling, streamed truncation/corruption/oversize/interruption, partial cleanup, verified reuse and overwrite refusal. The Windows targets are type/lint checked locally; native dialogs, folder reveal and live Windows x64/ARM64 behavior still require interactive validation on Windows. The existing CI workflow builds and tests both Windows architectures.
