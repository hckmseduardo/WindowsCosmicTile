//! Consent-gated release discovery and verified update staging.
use semver::Version;
use serde_json::Value;
#[derive(Clone, Debug)]
pub struct Release {
    pub version: Version,
    pub notes: String,
    url: String,
    size: u64,
    digest: String,
    machine: u16,
}
pub fn select(data: &Value, current: &str, arch: &str) -> Option<Release> {
    let machine = match arch {
        "x64" => 0x8664,
        "arm64" => 0xaa64,
        _ => return None,
    };
    let current = Version::parse(current).ok()?;
    data.as_array()?
        .iter()
        .filter_map(|item| {
            if item.get("draft")?.as_bool()? {
                return None;
            }
            let tag = item.get("tag_name")?.as_str()?;
            let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()?;
            if version <= current {
                return None;
            }
            let name = format!("WindowsCosmicTile-v{version}-windows-{arch}.exe");
            let url = format!(
                "https://github.com/hckmseduardo/WindowsCosmicTile/releases/download/{tag}/{name}"
            );
            let asset = item
                .get("assets")?
                .as_array()?
                .iter()
                .find(|a| a.get("name").and_then(Value::as_str) == Some(&name))?;
            let size = asset.get("size")?.as_u64()?;
            let digest = asset.get("digest")?.as_str()?.strip_prefix("sha256:")?;
            if asset.get("state")?.as_str()? != "uploaded"
                || size == 0
                || size > MAX_BYTES
                || digest.len() != 64
                || !digest.bytes().all(|b| b.is_ascii_hexdigit())
                || asset.get("browser_download_url")?.as_str()? != url
            {
                return None;
            }
            Some(Release {
                version,
                notes: item
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .chars()
                    .take(6000)
                    .filter(|c| *c != '\0')
                    .collect(),
                url,
                size,
                digest: digest.to_ascii_lowercase(),
                machine,
            })
        })
        .max_by(|a, b| a.version.cmp(&b.version))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn release(v: &str, arch: &str) -> Value {
        let name = format!("WindowsCosmicTile-v{v}-windows-{arch}.exe");
        json!({"tag_name":format!("v{v}"), "draft":false, "prerelease":true, "body":"notes", "assets":[{"name":name,"state":"uploaded","size":3,"digest":format!("sha256:{}", "a".repeat(64)),"browser_download_url":format!("https://github.com/hckmseduardo/WindowsCosmicTile/releases/download/v{v}/{name}")}]})
    }
    #[test]
    fn selection_uses_highest_semver_including_previews() {
        let data = json!([
            release("0.3.0-beta.1", "x64"),
            release("0.2.1", "x64"),
            release("0.10.0", "x64")
        ]);
        assert_eq!(
            select(&data, "0.2.0", "x64").unwrap().version.to_string(),
            "0.10.0"
        );
        assert_eq!(
            select(&json!([release("0.3.0-beta.1", "arm64")]), "0.2.0", "arm64")
                .unwrap()
                .version
                .to_string(),
            "0.3.0-beta.1"
        );
    }
    #[test]
    fn rejects_wrong_arch_repo_digest_state_size_draft_and_tags() {
        let valid = release("0.3.0", "x64");
        assert!(select(&json!([valid.clone()]), "0.2.0", "arm64").is_none());
        for (pointer, value) in [
            ("/draft", json!(true)),
            ("/tag_name", json!("v0.3")),
            ("/assets/0/state", json!("new")),
            ("/assets/0/size", json!(0)),
            ("/assets/0/size", json!(536870913u64)),
            ("/assets/0/digest", json!("sha256:abc")),
            ("/assets/0/digest", json!(format!("sha256:{}", "z".repeat(64)))),
            ("/assets/0/digest", json!(format!("sha512:{}", "a".repeat(64)))),
            ("/assets/0/name", json!("WindowsCosmicTile-v0.3.0-windows-x64.zip")),
            ("/assets/0/name", json!("../../other.exe")),
            ("/assets/0/browser_download_url", json!("http://github.com/hckmseduardo/WindowsCosmicTile/releases/download/v0.3.0/WindowsCosmicTile-v0.3.0-windows-x64.exe")),
            ("/assets/0/browser_download_url", json!("https://github.com/hckmseduardo/WindowsCosmicTile/releases/download/v0.3.0/WindowsCosmicTile-v0.3.0-windows-arm64.exe")),
            ("/assets/0/browser_download_url", json!("https://github.com/hckmseduardo/WindowsCosmicTile/releases/download/v0.3.0/WindowsCosmicTile-v0.3.0-windows-x64.exe?extra=true")),
            (
                "/assets/0/browser_download_url",
                json!("https://github.com/evil/WindowsCosmicTile/releases/download/v0.3.0/x.exe"),
            ),
        ] {
            let mut item = valid.clone();
            *item.pointer_mut(pointer).unwrap() = value;
            assert!(
                select(&json!([item]), "0.2.0", "x64").is_none(),
                "{pointer}"
            );
        }
        assert!(select(&json!([valid]), "0.3.0", "x64").is_none());
    }
}

use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const MAX_BYTES: u64 = 512 * 1024 * 1024;
pub const STAGING_PREFIX: &str = ".cosmictile-update-";
pub const STAGING_SUFFIX_LEN: usize = 12;
pub const REPLACEMENT_EXE: &str = "replacement.exe";
const API: &str =
    "https://api.github.com/repos/hckmseduardo/WindowsCosmicTile/releases?per_page=100";
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn due(auto: bool, last: u64, time: u64) -> bool {
    auto && (last == 0 || time.saturating_sub(last) >= 86400)
}
fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("WindowsCosmicTile/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let url = attempt.url();
            if attempt.previous().len() >= 5
                || url.scheme() != "https"
                || !matches!(
                    url.host_str(),
                    Some(
                        "github.com"
                            | "release-assets.githubusercontent.com"
                            | "objects.githubusercontent.com"
                    )
                )
            {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| e.to_string())
}
fn check() -> Result<Option<Release>, String> {
    let response = client()?
        .get(API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    response
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("Release metadata exceeds limit".into());
    }
    let data: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if !data.is_array() {
        return Err("Invalid release metadata".into());
    }
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        return Err("Unsupported CPU architecture".into());
    };
    Ok(select(&data, env!("CARGO_PKG_VERSION"), arch))
}
fn store(mut input: impl Read, release: &Release, path: &Path) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        let mut hash = Sha256::new();
        let mut total = 0u64;
        let mut buf = [0u8; 65536];
        loop {
            let count = input.read(&mut buf).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            total += count as u64;
            if total > release.size || total > MAX_BYTES {
                return Err("Download exceeds advertised size".into());
            }
            file.write_all(&buf[..count]).map_err(|e| e.to_string())?;
            hash.update(&buf[..count]);
        }
        if total != release.size || format!("{:x}", hash.finalize()) != release.digest {
            return Err("Download size or SHA-256 does not match GitHub metadata".into());
        }
        file.sync_all().map_err(|e| e.to_string())
    })();
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}
fn check_executable(mut executable: impl Read + Seek, machine: u16) -> std::io::Result<()> {
    let invalid = || {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Update is not a Windows executable for this CPU",
        )
    };
    let mut dos = [0; 64];
    executable.read_exact(&mut dos)?;
    let offset = u32::from_le_bytes([dos[60], dos[61], dos[62], dos[63]]);
    if !dos.starts_with(b"MZ") || offset < 64 {
        return Err(invalid());
    }
    executable.seek(SeekFrom::Start(u64::from(offset)))?;
    let mut pe = [0; 6];
    executable.read_exact(&mut pe)?;
    if !pe.starts_with(b"PE\0\0") || u16::from_le_bytes([pe[4], pe[5]]) != machine {
        return Err(invalid());
    }
    Ok(())
}
#[derive(Debug)]
pub struct StagedUpdate {
    pub directory: tempfile::TempDir,
    pub executable: PathBuf,
}

fn download(release: &Release, executable: PathBuf) -> Result<StagedUpdate, String> {
    let parent = executable
        .parent()
        .ok_or("Cannot locate the installation folder")?;
    let directory = tempfile::Builder::new()
        .prefix(STAGING_PREFIX)
        .rand_bytes(STAGING_SUFFIX_LEN)
        .tempdir_in(parent)
        .map_err(|e| e.to_string())?;
    let response = client()?
        .get(&release.url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?;
    if response.content_length().is_some_and(|n| n != release.size) {
        return Err("Unexpected HTTP content length".into());
    }
    let candidate = directory.path().join(REPLACEMENT_EXE);
    store(response, release, &candidate)?;
    let file = File::open(candidate).map_err(|e| e.to_string())?;
    check_executable(file, release.machine).map_err(|e| e.to_string())?;
    Ok(StagedUpdate {
        directory,
        executable,
    })
}
#[derive(Debug)]
pub enum Event {
    Checked {
        manual: bool,
        result: Result<Option<Release>, String>,
    },
    Downloaded(Result<StagedUpdate, String>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Checking,
    AwaitingConsent,
    Downloading,
}
pub struct Updater {
    phase: Phase,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    pending: Option<Release>,
}
impl Default for Updater {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            phase: Phase::Idle,
            tx,
            rx,
            pending: None,
        }
    }
}
impl Updater {
    pub fn check(&mut self, manual: bool) -> bool {
        if self.phase != Phase::Idle {
            return false;
        }
        self.phase = Phase::Checking;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Checked {
                manual,
                result: check(),
            });
        });
        true
    }
    pub fn poll(&mut self) -> Option<Event> {
        let event = self.rx.try_recv().ok()?;
        match &event {
            Event::Checked {
                result: Ok(Some(release)),
                ..
            } => {
                self.phase = Phase::AwaitingConsent;
                self.pending = Some(release.clone());
            }
            _ => self.phase = Phase::Idle,
        }
        Some(event)
    }
    /// Only a direct native-dialog Yes response may supply an executable path.
    pub fn consent(&mut self, executable: Option<PathBuf>) {
        if self.phase != Phase::AwaitingConsent {
            return;
        }
        let release = self.pending.take();
        self.phase = Phase::Idle;
        if let (Some(release), Some(executable)) = (release, executable) {
            self.phase = Phase::Downloading;
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Event::Downloaded(download(&release, executable)));
            });
        }
    }
}

#[cfg(test)]
mod integrity_tests {
    use super::*;
    use std::io::Cursor;
    fn candidate() -> Release {
        Release {
            version: Version::parse("0.3.0").unwrap(),
            notes: String::new(),
            url: "https://github.com/hckmseduardo/WindowsCosmicTile/releases/download/v0.3.0/WindowsCosmicTile-v0.3.0-windows-x64.exe".into(),
            size: 3,
            digest: format!("{:x}", Sha256::digest(b"abc")),
            machine: 0x8664,
        }
    }
    #[test]
    fn verified_candidate_is_written_without_overwriting_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(REPLACEMENT_EXE);
        let r = candidate();
        store(Cursor::new(b"abc"), &r, &path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"abc");
        assert!(store(Cursor::new(b"xyz"), &r, &path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"abc");
    }
    #[test]
    fn truncated_oversized_corrupt_and_interrupted_streams_leave_no_files() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("interrupted"))
            }
        }
        let r = candidate();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(REPLACEMENT_EXE);
        for bytes in [b"ab".as_slice(), b"abcd", b"xyz"] {
            assert!(store(Cursor::new(bytes), &r, &path).is_err());
            assert!(!path.exists());
        }
        assert!(store(Broken, &r, &path).is_err());
        assert!(!path.exists());
    }
    #[test]
    fn executable_header_must_match_the_running_architecture() {
        let mut bytes = [0; 70];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
        bytes[64..70].copy_from_slice(b"PE\0\0\x64\x86");
        assert!(check_executable(Cursor::new(bytes), 0x8664).is_ok());
        assert!(check_executable(Cursor::new(bytes), 0xaa64).is_err());
        assert!(check_executable(Cursor::new(b"not an executable"), 0x8664).is_err());
    }
    #[test]
    fn decline_and_stale_consent_never_start_download_or_create_folder() {
        let mut updater = Updater::default();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("installed.exe");
        updater.consent(Some(path.clone()));
        assert_eq!(updater.phase, Phase::Idle);
        updater.phase = Phase::Checking;
        assert!(!updater.check(true));
        updater
            .tx
            .send(Event::Checked {
                manual: false,
                result: Ok(Some(candidate())),
            })
            .unwrap();
        assert!(matches!(updater.poll(), Some(Event::Checked { .. })));
        assert_eq!(updater.phase, Phase::AwaitingConsent);
        assert!(!updater.check(true));
        updater.consent(None);
        assert_eq!(updater.phase, Phase::Idle);
        assert!(updater.pending.is_none());
        updater.consent(Some(path));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
        assert!(updater.poll().is_none());
    }
    #[test]
    fn daily_check_honors_disabled_and_clock_rollback() {
        assert!(due(true, 0, 1));
        assert!(!due(false, 0, 100000));
        assert!(!due(true, 100000, 1));
        assert!(!due(true, 1, 86400));
        assert!(due(true, 1, 86401));
    }
}

/// Compact release-note text suitable for a native confirmation dialog.
pub fn note_summary(notes: &str) -> String {
    let notes = notes.trim();
    let lines = notes.lines().take(12).collect::<Vec<_>>().join("\n");
    let mut summary: String = lines.chars().take(1200).collect();
    if summary != notes {
        summary.push('…');
    }
    summary
}
#[cfg(test)]
mod summary_tests {
    use super::note_summary;
    #[test]
    fn native_summary_limits_unicode_characters_and_line_count() {
        assert_eq!(note_summary("Brief release notes"), "Brief release notes");
        let summary = note_summary(&"é".repeat(1400));
        assert!(summary.chars().count() <= 1201);
        assert!(summary.ends_with('…'));
        let summary = note_summary(&"line\n".repeat(30));
        assert!(summary.lines().count() <= 12);
        assert!(summary.ends_with('…'));
    }
}
