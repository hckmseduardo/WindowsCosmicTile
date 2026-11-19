use windows_sys::Win32::System::Power::{
    SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
};

/// Owned by the UI thread, which must also release its execution request.
#[derive(Default)]
pub struct KeepAwake {
    enabled: bool,
}
impl KeepAwake {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn toggle(&mut self) -> Result<(), std::io::Error> {
        let next = !self.enabled;
        let flags = ES_CONTINUOUS
            | if next {
                ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED
            } else {
                0
            };
        if unsafe { SetThreadExecutionState(flags) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        self.enabled = next;
        Ok(())
    }
    pub fn disable(&mut self) {
        if self.enabled {
            let _ = self.toggle();
        }
    }
}
impl Drop for KeepAwake {
    fn drop(&mut self) {
        self.disable();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_request_is_reversible() {
        let mut awake = KeepAwake::default();
        assert!(!awake.enabled());
        awake.toggle().unwrap();
        assert!(awake.enabled());
        awake.toggle().unwrap();
        assert!(!awake.enabled());
    }
}
