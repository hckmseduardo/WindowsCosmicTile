//! Hands the verified executable to a temporary copy of the running app.
use super::desktop::wide;
use crate::installer::{replace_and_restart, InstallError};
use cosmitile::{
    i18n::Language,
    updates::{StagedUpdate, REPLACEMENT_EXE, STAGING_PREFIX, STAGING_SUFFIX_LEN},
};
use std::{
    collections::BTreeSet,
    ffi::{OsStr, OsString},
    fs, io,
    os::windows::{
        ffi::OsStrExt,
        fs::MetadataExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    process::Command,
    ptr::{null, null_mut},
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0,
        WAIT_TIMEOUT,
    },
    Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
    Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT,
    System::Diagnostics::Debug::{GetErrorMode, SetErrorMode, SEM_FAILCRITICALERRORS},
    System::Threading::{
        CreateEventW, CreateMutexW, GetCurrentProcess, OpenEventW, OpenProcessToken, ReleaseMutex,
        SetEvent, WaitForMultipleObjects, WaitForSingleObject, EVENT_MODIFY_STATE,
    },
    UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK},
};

const PREVIOUS_EXE: &str = "previous.exe";
const ACTIVE_DISPLAYS: &str = "active-displays.json";
const APPLY: &str = "--apply-update";
const FINISH: &str = "--finish-update";
const RECOVERY_MESSAGE: &str = "COSMICTILE_UPDATE_RECOVERY_MESSAGE";
pub const HANDOFF_TIMEOUT_MS: u32 = 30_000;

pub struct Instance {
    handle: OwnedHandle,
    owned: bool,
}

impl Instance {
    pub fn acquire(timeout_ms: u32) -> io::Result<Self> {
        unsafe {
            let mutex = CreateMutexW(
                null(),
                0,
                wide("Local\\WindowsCosmicTile.SingleInstance").as_ptr(),
            );
            if mutex.is_null() {
                return Err(io::Error::last_os_error());
            }
            if timeout_ms == 0 && GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(mutex);
                return Err(io::Error::from(io::ErrorKind::WouldBlock));
            }
            let mut instance = Self {
                handle: OwnedHandle::from_raw_handle(mutex),
                owned: false,
            };
            instance.lock(timeout_ms)?;
            Ok(instance)
        }
    }

    fn lock(&mut self, timeout_ms: u32) -> io::Result<()> {
        match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), timeout_ms) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => {
                self.owned = true;
                Ok(())
            }
            WAIT_TIMEOUT => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "CosmicTile is still running",
            )),
            _ => Err(io::Error::last_os_error()),
        }
    }

    fn unlock(&mut self) {
        if self.owned {
            self.owned = unsafe { ReleaseMutex(self.handle.as_raw_handle()) } == 0;
        }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.unlock();
    }
}

pub fn can_install() -> bool {
    unsafe {
        let mut token = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let token = OwnedHandle::from_raw_handle(token);
        let mut elevation = TOKEN_ELEVATION::default();
        let mut size = 0;
        GetTokenInformation(
            token.as_raw_handle(),
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        ) != 0
            && elevation.TokenIsElevated == 0
    }
}

pub fn start(
    update: StagedUpdate,
    language: Language,
    active_displays: &BTreeSet<String>,
) -> io::Result<()> {
    let name = update
        .executable
        .file_name()
        .filter(|name| is_executable_name(name))
        .ok_or(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid executable name",
        ))?;
    let stage = update.directory.path();
    if !active_displays.is_empty() {
        fs::write(
            stage.join(ACTIVE_DISPLAYS),
            serde_json::to_vec(active_displays)?,
        )?;
    }
    let helper = stage.join(PREVIOUS_EXE);
    fs::copy(&update.executable, &helper)?;
    Command::new(helper)
        .arg(APPLY)
        .arg(name)
        .arg(language.code())
        .spawn()?;
    // The helper and restarted app now own cleanup of this directory.
    let _ = update.directory.keep();
    Ok(())
}

pub fn dispatch(arguments: &[OsString]) -> Option<i32> {
    if arguments.first()?.as_os_str() != OsStr::new(APPLY) {
        return None;
    }
    let [_, executable, language] = arguments else {
        return Some(1);
    };
    // Let the updater handle loader errors and roll back without waiting for
    // someone to dismiss a Windows critical-error dialog.
    unsafe { SetErrorMode(GetErrorMode() | SEM_FAILCRITICALERRORS) };
    let language = Language::from_code(language.to_str().unwrap_or_default());
    Some(match apply(executable) {
        Ok(()) => 0,
        Err(error) => {
            show_error(language, &error.to_string());
            1
        }
    })
}

pub struct Restart {
    signal: OwnedHandle,
    stage: PathBuf,
    pub active_displays: BTreeSet<String>,
}

pub fn restart_stage(arguments: &[OsString]) -> Option<Restart> {
    let [flag, stage] = arguments else {
        return None;
    };
    if flag != FINISH || !is_staging_name(stage) {
        return None;
    }
    let executable = std::env::current_exe().ok()?;
    let directory = executable.parent()?.join(stage);
    if !plain_directory(&directory) {
        return None;
    }
    let signal = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, ready_event_name(stage).as_ptr()) };
    (!signal.is_null()).then(|| Restart {
        signal: unsafe { OwnedHandle::from_raw_handle(signal) },
        active_displays: fs::read(directory.join(ACTIVE_DISPLAYS))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default(),
        stage: directory,
    })
}

fn is_executable_name(name: &OsStr) -> bool {
    let path = Path::new(name);
    path.file_name() == Some(name)
        && path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        && !name.encode_wide().any(|ch| ch == b':' as u16)
}

fn is_staging_name(name: &OsStr) -> bool {
    name.to_str()
        .and_then(|name| name.strip_prefix(STAGING_PREFIX))
        .is_some_and(|suffix| {
            suffix.len() == STAGING_SUFFIX_LEN && suffix.bytes().all(|b| b.is_ascii_alphanumeric())
        })
}

fn plain_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_dir() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
    })
}

fn ready_event_name(stage: &OsStr) -> Vec<u16> {
    wide(&format!(
        "Local\\WindowsCosmicTile.Ready.{}",
        stage.to_string_lossy()
    ))
}

fn apply(executable: &OsStr) -> io::Result<()> {
    let helper = std::env::current_exe()?;
    let stage = helper
        .parent()
        .filter(|path| path.file_name().is_some_and(is_staging_name) && plain_directory(path));
    let (stage, stage_name, install_folder) = stage
        .and_then(|path| Some((path, path.file_name()?, path.parent()?)))
        .filter(|_| {
            helper.file_name() == Some(OsStr::new(PREVIOUS_EXE)) && is_executable_name(executable)
        })
        .ok_or(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid update directory or executable",
        ))?;
    let installed = install_folder.join(executable);
    let mut instance = Instance::acquire(HANDOFF_TIMEOUT_MS)?;
    match replace_and_restart(&installed, &stage.join(REPLACEMENT_EXE), &helper, |path| {
        launch_and_wait(&mut instance, path, stage_name, None)
    }) {
        Ok(()) => Ok(()),
        Err(error) if error.recovery_error.is_none() => launch_and_wait(
            &mut instance,
            &installed,
            stage_name,
            Some(&error.error.to_string()),
        )
        .map_err(|error| error.error),
        Err(error) => Err(io::Error::other(format!(
            "{}\n{}\n{}",
            error.error,
            error.recovery_error.expect("rollback failed"),
            helper.display(),
        ))),
    }
}

fn launch_and_wait(
    instance: &mut Instance,
    path: &Path,
    stage: &OsStr,
    recovery_message: Option<&str>,
) -> Result<(), InstallError> {
    let signal = unsafe {
        let handle = CreateEventW(null(), 1, 0, ready_event_name(stage).as_ptr());
        if handle.is_null() {
            return Err(io::Error::last_os_error().into());
        }
        let already_exists = GetLastError() == ERROR_ALREADY_EXISTS;
        let handle = OwnedHandle::from_raw_handle(handle);
        if already_exists {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Update is already restarting",
            )
            .into());
        }
        handle
    };
    let mut command = Command::new(path);
    command.arg(FINISH).arg(stage).env_remove(RECOVERY_MESSAGE);
    if let Some(message) = recovery_message {
        command.env(RECOVERY_MESSAGE, message);
    }
    let mut child = command.spawn()?;
    // Keep the mutex handle alive so ordinary launches still see a running
    // instance. Only the restarted app is allowed to take its ownership.
    instance.unlock();
    let handles: [HANDLE; 2] = [signal.as_raw_handle(), child.as_raw_handle()];
    let error = match unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, HANDOFF_TIMEOUT_MS) }
    {
        WAIT_OBJECT_0 => return Ok(()),
        result if result == WAIT_OBJECT_0 + 1 => {
            io::Error::other("CosmicTile exited before its tray initialized")
        }
        WAIT_TIMEOUT => io::Error::new(
            io::ErrorKind::TimedOut,
            "CosmicTile did not finish restarting",
        ),
        _ => io::Error::last_os_error(),
    };
    let _ = child.kill();
    let recovery_error =
        match unsafe { WaitForSingleObject(child.as_raw_handle(), HANDOFF_TIMEOUT_MS) } {
            WAIT_OBJECT_0 if instance.owned => None,
            WAIT_OBJECT_0 => instance.lock(HANDOFF_TIMEOUT_MS).err(),
            _ => Some(io::Error::other("The replacement process did not exit")),
        };
    Err(InstallError {
        error,
        recovery_error,
    })
}

pub fn finish_restart(restart: Restart, language: Language) {
    if unsafe { SetEvent(restart.signal.as_raw_handle()) } == 0 {
        return;
    }
    let stage = restart.stage;
    std::thread::spawn(move || {
        if !plain_directory(&stage) {
            return;
        }
        let helper = stage.join(PREVIOUS_EXE);
        // The helper releases the instance mutex immediately before exiting.
        for _ in 0..30 {
            match fs::remove_file(&helper) {
                Ok(()) => break,
                Err(error) if error.kind() == io::ErrorKind::NotFound => break,
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            }
        }
        let _ = fs::remove_file(stage.join(REPLACEMENT_EXE));
        let _ = fs::remove_file(stage.join(ACTIVE_DISPLAYS));
        let _ = fs::remove_dir(stage);
    });
    if let Ok(message) = std::env::var(RECOVERY_MESSAGE) {
        show_error(language, &message);
    }
}

fn show_error(language: Language, details: &str) {
    let text = format!(
        "{}\n\n{details}",
        language.text("Could not install the update. Please try again.")
    );
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            wide(&text).as_ptr(),
            wide("CosmicTile").as_ptr(),
            MB_ICONERROR | MB_OK,
        );
    }
}
