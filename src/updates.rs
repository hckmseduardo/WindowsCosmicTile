//! Consent-gated release discovery and verified download support.
use semver::Version;
use serde_json::Value;
#[derive(Clone, Debug)]
pub struct Release {
    pub version: Version,
    pub notes: String,
    name: String,
    url: String,
    size: u64,
    digest: String,
}
pub fn select(data: &Value, current: &str, arch: &str) -> Option<Release> {
    if !matches!(arch, "x64" | "arm64") {
        return None;
    }
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
            let name = format!("WindowsCosmicTile-v{version}-windows-{arch}.zip");
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
                name,
                url,
                size,
                digest: digest.to_ascii_lowercase(),
            })
        })
        .max_by(|a, b| a.version.cmp(&b.version))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn release(v: &str, arch: &str) -> Value {
        let name = format!("WindowsCosmicTile-v{v}-windows-{arch}.zip");
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
            ("/assets/0/name", json!("../../other.zip")),
            ("/assets/0/browser_download_url", json!("http://github.com/hckmseduardo/WindowsCosmicTile/releases/download/v0.3.0/WindowsCosmicTile-v0.3.0-windows-x64.zip")),
            ("/assets/0/browser_download_url", json!("https://github.com/hckmseduardo/WindowsCosmicTile/releases/download/v0.3.0/WindowsCosmicTile-v0.3.0-windows-arm64.zip")),
            ("/assets/0/browser_download_url", json!("https://github.com/hckmseduardo/WindowsCosmicTile/releases/download/v0.3.0/WindowsCosmicTile-v0.3.0-windows-x64.zip?extra=true")),
            (
                "/assets/0/browser_download_url",
                json!("https://github.com/evil/WindowsCosmicTile/releases/download/v0.3.0/x.zip"),
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
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const MAX_BYTES: u64 = 512 * 1024 * 1024;
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
fn verify(path: &Path, release: &Release) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    if file.metadata().map(|m| m.len()).ok() != Some(release.size) {
        return false;
    }
    let mut hash = Sha256::new();
    let mut limited = file.take(release.size + 1);
    std::io::copy(&mut limited, &mut hash).is_ok()
        && format!("{:x}", hash.finalize()) == release.digest
}
fn store(mut input: impl Read, release: &Release, folder: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let final_path = folder.join(&release.name);
    if final_path.exists() {
        return if verify(&final_path, release) {
            Ok(final_path)
        } else {
            Err("An existing download has different contents; move it before retrying.".into())
        };
    }
    let mut partial = None;
    for n in 0..100u32 {
        let path = folder.join(format!(
            ".{}-{}-{}-{n}.part",
            release.name,
            std::process::id(),
            now()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                partial = Some((path, file));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.to_string()),
        }
    }
    let (path, mut file) = partial.ok_or("Cannot create a unique partial download")?;
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
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        // Hard-link publishes a complete file atomically without replacing an existing file.
        match fs::hard_link(&path, &final_path) {
            Ok(()) => Ok(final_path.clone()),
            Err(_) if verify(&final_path, release) => Ok(final_path.clone()),
            Err(e) => Err(e.to_string()),
        }
    })();
    let _ = fs::remove_file(path);
    result
}
fn download(release: &Release, folder: &Path) -> Result<PathBuf, String> {
    if verify(&folder.join(&release.name), release) {
        return Ok(folder.join(&release.name));
    }
    let response = client()?
        .get(&release.url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?;
    if response.content_length().is_some_and(|n| n != release.size) {
        return Err("Unexpected HTTP content length".into());
    }
    store(response, release, folder)
}
#[derive(Debug)]
pub enum Event {
    Checked {
        manual: bool,
        result: Result<Option<Release>, String>,
    },
    Downloaded(Result<PathBuf, String>),
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
    /// Only a direct native-dialog Yes response may pass `true` here.
    pub fn consent(&mut self, approved: bool, folder: PathBuf) {
        if self.phase != Phase::AwaitingConsent {
            return;
        }
        let release = self.pending.take();
        self.phase = Phase::Idle;
        if !approved {
            return;
        }
        if let Some(release) = release {
            self.phase = Phase::Downloading;
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Event::Downloaded(download(&release, &folder)));
            });
        }
    }
}

#[cfg(test)]
mod integrity_tests {
    use super::*;
    use std::io::Cursor;
    fn candidate() -> Release {
        Release { version:Version::parse("0.3.0").unwrap(), notes:String::new(), name:"WindowsCosmicTile-v0.3.0-windows-x64.zip".into(),url:"https://github.com/hckmseduardo/WindowsCosmicTile/releases/download/v0.3.0/WindowsCosmicTile-v0.3.0-windows-x64.zip".into(),size:3,digest:format!("{:x}",Sha256::digest(b"abc")) }
    }
    fn directory() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "cosmitile-update-test-{}-{}-{}",
            std::process::id(),
            now(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }
    #[test]
    fn verified_files_are_reused_and_never_overwritten() {
        let dir = directory();
        let r = candidate();
        let path = store(Cursor::new(b"abc"), &r, &dir).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"abc");
        assert_eq!(store(Cursor::new(b"unused"), &r, &dir).unwrap(), path);
        fs::write(&path, b"old").unwrap();
        assert!(store(Cursor::new(b"abc"), &r, &dir).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"old");
        fs::remove_dir_all(dir).unwrap();
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
        for bytes in [b"ab".as_slice(), b"abcd", b"xyz"] {
            let dir = directory();
            assert!(store(Cursor::new(bytes), &r, &dir).is_err());
            assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
            fs::remove_dir_all(dir).unwrap();
        }
        let dir = directory();
        assert!(store(Broken, &r, &dir).is_err());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn decline_and_stale_consent_never_start_download_or_create_folder() {
        let mut updater = Updater::default();
        let dir = directory();
        updater.consent(true, dir.clone());
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
        updater.consent(false, dir.clone());
        assert_eq!(updater.phase, Phase::Idle);
        assert!(updater.pending.is_none());
        updater.consent(true, dir.clone());
        assert!(!dir.exists());
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
