use std::{fs, io, path::Path};

#[derive(Debug)]
pub struct InstallError {
    pub error: io::Error,
    pub recovery_error: Option<io::Error>,
}

impl From<io::Error> for InstallError {
    fn from(error: io::Error) -> Self {
        Self {
            error,
            recovery_error: None,
        }
    }
}

pub fn replace_and_restart(
    installed: &Path,
    replacement: &Path,
    backup: &Path,
    launch: impl FnOnce(&Path) -> Result<(), InstallError>,
) -> Result<(), InstallError> {
    replace(replacement, installed)?;
    launch(installed).map_err(|mut failure| {
        if failure.recovery_error.is_none() {
            let restored = backup.with_file_name("restore.exe");
            failure.recovery_error = fs::copy(backup, &restored)
                .and_then(|_| replace(&restored, installed))
                .err();
        }
        failure
    })
}

fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    // The GUI has released its instance mutex, but the Windows loader may
    // still be unmapping the old image. Retry only file-sharing errors.
    #[cfg(windows)]
    for _ in 0..20 {
        match fs::rename(source, destination) {
            Err(error) if matches!(error.raw_os_error(), Some(5 | 32 | 33)) => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            result => return result,
        }
    }
    fs::rename(source, destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io};

    #[test]
    fn replaces_the_original_path_before_relaunching() {
        let directory = tempfile::tempdir().unwrap();
        let installed = directory.path().join("My CosmicTile.exe");
        let replacement = directory.path().join("replacement.exe");
        let backup = directory.path().join("previous.exe");
        fs::write(&installed, b"old").unwrap();
        fs::write(&replacement, b"new").unwrap();
        fs::write(&backup, b"old").unwrap();
        replace_and_restart(&installed, &replacement, &backup, |path| {
            assert_eq!(path, installed);
            assert_eq!(fs::read(path)?, b"new");
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read(&installed).unwrap(), b"new");
        assert_eq!(fs::read(&backup).unwrap(), b"old");
    }

    #[test]
    fn restores_the_previous_executable_if_launch_fails() {
        let directory = tempfile::tempdir().unwrap();
        let installed = directory.path().join("My CosmicTile.exe");
        let replacement = directory.path().join("replacement.exe");
        let backup = directory.path().join("previous.exe");
        fs::write(&installed, b"old").unwrap();
        fs::write(&replacement, b"new").unwrap();
        fs::write(&backup, b"old").unwrap();
        let error = replace_and_restart(&installed, &replacement, &backup, |_| {
            Err(io::Error::other("start failed").into())
        })
        .unwrap_err();
        assert_eq!(error.error.to_string(), "start failed");
        assert!(error.recovery_error.is_none());
        assert_eq!(fs::read(&installed).unwrap(), b"old");
    }

    #[test]
    fn leaves_the_original_in_place_if_replacement_fails() {
        let directory = tempfile::tempdir().unwrap();
        let installed = directory.path().join("My CosmicTile.exe");
        let replacement = directory.path().join("missing.exe");
        let backup = directory.path().join("previous.exe");
        fs::write(&installed, b"old").unwrap();
        fs::write(&backup, b"old").unwrap();
        let error = replace_and_restart(&installed, &replacement, &backup, |_| {
            panic!("must not launch after a failed replacement")
        })
        .unwrap_err();
        assert_eq!(error.error.kind(), io::ErrorKind::NotFound);
        assert!(error.recovery_error.is_none());
        assert_eq!(fs::read(&installed).unwrap(), b"old");
    }
}
