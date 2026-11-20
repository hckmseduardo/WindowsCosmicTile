//! Small unsafe adapter: HWNDs are discovered and used only on the UI thread.
use cosmitile::core::Rect;
use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::{
    core::BOOL,
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::Threading::GetCurrentProcessId,
        UI::WindowsAndMessaging::*,
    },
};

// Win32 encodes MAKEINTRESOURCE(1) in a pointer-sized value; it is never dereferenced.
pub const APP_ICON_RESOURCE: *const u16 = std::ptr::without_provenance::<u16>(1);

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
pub fn from_rect(r: RECT) -> Rect {
    Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Monitor {
    pub id: String,
    pub work: Rect,
}
#[derive(Clone)]
pub struct Window {
    pub id: isize,
    pub frame: Rect,
    pub pid: u32,
}
#[derive(Clone)]
pub struct Original {
    pub placement: WINDOWPLACEMENT,
    pub pid: u32,
    pub monitor: String,
}

pub unsafe fn monitors() -> Vec<Monitor> {
    unsafe extern "system" fn callback(
        handle: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let result = &mut *(data as *mut Vec<Monitor>);
        let mut info: MONITORINFOEXW = zeroed();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(handle, &mut info.monitorInfo) != 0 {
            let end = info
                .szDevice
                .iter()
                .position(|v| *v == 0)
                .unwrap_or(info.szDevice.len());
            result.push(Monitor {
                id: String::from_utf16_lossy(&info.szDevice[..end]),
                work: from_rect(info.monitorInfo.rcWork),
            });
        }
        1
    }
    let mut result: Vec<Monitor> = Vec::new();
    EnumDisplayMonitors(
        null_mut(),
        null(),
        Some(callback),
        &mut result as *mut _ as LPARAM,
    );
    result.sort_by_key(|monitor| (monitor.work.x, monitor.work.y, monitor.id.clone()));
    result
}
pub unsafe fn raw_frame(hwnd: HWND) -> Option<Rect> {
    let mut raw: RECT = zeroed();
    if GetWindowRect(hwnd, &mut raw) != 0 {
        Some(from_rect(raw))
    } else {
        None
    }
}
pub unsafe fn visible_frame(hwnd: HWND) -> Option<Rect> {
    let mut visible: RECT = zeroed();
    if DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS as u32,
        &mut visible as *mut _ as _,
        size_of::<RECT>() as u32,
    ) >= 0
        && visible.right > visible.left
        && visible.bottom > visible.top
    {
        Some(from_rect(visible))
    } else {
        raw_frame(hwnd)
    }
}
pub unsafe fn pid(hwnd: HWND) -> u32 {
    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    pid
}
fn window_styles_eligible(style: u32, extended: u32, class_name: &str) -> bool {
    // Some windows draw their own resize borders but still support maximizing.
    let maximizable = style & (WS_SYSMENU | WS_MAXIMIZEBOX) == (WS_SYSMENU | WS_MAXIMIZEBOX);
    cosmitile::core::window_eligible(
        style & WS_CHILD != 0,
        style & WS_THICKFRAME != 0 || maximizable,
        extended & (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_DLGMODALFRAME) != 0,
        class_name,
    )
}
pub unsafe fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked: u32 = 0;
    DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED as u32,
        &mut cloaked as *mut _ as _,
        size_of::<u32>() as u32,
    );
    cloaked != 0
}
pub unsafe fn windows() -> Vec<Window> {
    unsafe extern "system" fn callback(hwnd: HWND, data: LPARAM) -> BOOL {
        let result = &mut *(data as *mut Vec<Window>);
        if IsWindowVisible(hwnd) == 0
            || IsIconic(hwnd) != 0
            || pid(hwnd) == GetCurrentProcessId()
            || !GetWindow(hwnd, GW_OWNER).is_null()
        {
            return 1;
        }
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let extended = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let mut name = [0u16; 128];
        let len = GetClassNameW(hwnd, name.as_mut_ptr(), name.len() as i32).max(0) as usize;
        let class = String::from_utf16_lossy(&name[..len]);
        if !window_styles_eligible(style, extended, &class) || is_cloaked(hwnd) {
            return 1;
        }
        if let Some(frame) = visible_frame(hwnd) {
            if frame.w > 0 && frame.h > 0 {
                result.push(Window {
                    id: hwnd as isize,
                    frame,
                    pid: pid(hwnd),
                });
            }
        }
        1
    }
    let mut result: Vec<Window> = Vec::new();
    EnumWindows(Some(callback), &mut result as *mut _ as LPARAM);
    result.sort_by_key(|window| (window.frame.x, window.frame.y, window.id));
    result
}
pub fn monitor_for(monitors: &[Monitor], rect: Rect) -> Option<String> {
    monitors
        .iter()
        .max_by_key(|monitor| monitor.work.overlap(rect))
        .map(|monitor| monitor.id.clone())
}
pub unsafe fn snapshot(window: &Window, monitor: String) -> Option<Original> {
    let mut placement: WINDOWPLACEMENT = zeroed();
    placement.length = size_of::<WINDOWPLACEMENT>() as u32;
    if GetWindowPlacement(window.id as HWND, &mut placement) == 0 {
        return None;
    }
    Some(Original {
        placement,
        pid: window.pid,
        monitor,
    })
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PositionStatus {
    Queued,
    Restoring,
    Failed,
}
pub unsafe fn set_frame(id: isize, frame: Rect) -> PositionStatus {
    let hwnd = id as HWND;
    if IsWindow(hwnd) == 0 {
        return PositionStatus::Failed;
    }
    if IsZoomed(hwnd) != 0 {
        // Wait for restoration before measuring nonclient borders: maximized
        // offsets differ from normal-window offsets.
        return if ShowWindowAsync(hwnd, SW_RESTORE) != 0 {
            PositionStatus::Restoring
        } else {
            PositionStatus::Failed
        };
    }
    let Some(raw) = raw_frame(hwnd) else {
        return PositionStatus::Failed;
    };
    let visible = visible_frame(hwnd).unwrap_or(raw);
    // DWM excludes invisible resize borders, unlike GetWindowRect.
    let left = visible.x - raw.x;
    let top = visible.y - raw.y;
    let right = raw.right() - visible.right();
    let bottom = raw.bottom() - visible.bottom();
    let queued = SetWindowPos(
        hwnd,
        null_mut(),
        frame.x - left,
        frame.y - top,
        (frame.w + left + right).max(1),
        (frame.h + top + bottom).max(1),
        SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS,
    ) != 0;
    if queued {
        PositionStatus::Queued
    } else {
        PositionStatus::Failed
    }
}
fn restore_placement(mut original: WINDOWPLACEMENT, minimized: bool) -> WINDOWPLACEMENT {
    original.flags |= WPF_ASYNCWINDOWPLACEMENT;
    if minimized {
        if original.showCmd == SW_SHOWMAXIMIZED as u32 {
            original.flags |= WPF_RESTORETOMAXIMIZED;
            original.showCmd = SW_SHOWMINIMIZED as u32;
        } else {
            original.flags &= !WPF_RESTORETOMAXIMIZED;
            original.showCmd = SW_SHOWMINNOACTIVE as u32;
        }
    }
    original
}
pub unsafe fn monitor_id(hwnd: HWND) -> Option<String> {
    let handle = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL);
    let mut info: MONITORINFOEXW = zeroed();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if handle.is_null() || GetMonitorInfoW(handle, &mut info.monitorInfo) == 0 {
        return None;
    }
    let end = info
        .szDevice
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(info.szDevice.len());
    Some(String::from_utf16_lossy(&info.szDevice[..end]))
}
pub unsafe fn restore(id: isize, original: &Original, monitors: &[Monitor]) {
    let hwnd = id as HWND;
    if IsWindow(hwnd) == 0 || pid(hwnd) != original.pid {
        return;
    }
    if monitors
        .iter()
        .any(|monitor| monitor.id == original.monitor)
    {
        // WINDOWPLACEMENT retains workspace coordinates, minimized state, and
        // restore-to-maximized behavior. Windows adjusts obsolete work areas.
        let placement = restore_placement(original.placement, IsIconic(hwnd) != 0);
        SetWindowPlacement(hwnd, &placement);
    }
}
pub unsafe fn focus(id: isize) {
    let hwnd = id as HWND;
    if IsWindow(hwnd) != 0 {
        SetForegroundWindow(hwnd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_chrome_with_a_maximize_button_is_eligible_for_tiling() {
        let custom_chrome = WS_POPUP | WS_SYSMENU | WS_MAXIMIZEBOX;
        assert!(window_styles_eligible(custom_chrome, 0, "CustomChrome"));
        assert!(!window_styles_eligible(WS_POPUP, 0, "FixedPopup"));
        assert!(!window_styles_eligible(custom_chrome, 0, "#32770"));
        assert!(!window_styles_eligible(
            custom_chrome,
            WS_EX_TOOLWINDOW,
            "CustomChrome"
        ));
    }
    #[test]
    fn minimized_restore_preserves_saved_normal_position() {
        let mut original: WINDOWPLACEMENT = unsafe { zeroed() };
        original.showCmd = SW_SHOWNORMAL as u32;
        original.rcNormalPosition = RECT {
            left: 100,
            top: 120,
            right: 900,
            bottom: 720,
        };
        let restored = restore_placement(original, true);
        assert_eq!(restored.showCmd, SW_SHOWMINNOACTIVE as u32);
        assert_eq!(restored.rcNormalPosition.left, 100);
        assert_eq!(restored.rcNormalPosition.right, 900);
        assert_ne!(restored.flags & WPF_ASYNCWINDOWPLACEMENT, 0);
    }
    #[test]
    fn minimized_originally_maximized_window_restores_to_maximized_later() {
        let mut original: WINDOWPLACEMENT = unsafe { zeroed() };
        original.showCmd = SW_SHOWMAXIMIZED as u32;
        let restored = restore_placement(original, true);
        assert_ne!(restored.flags & WPF_RESTORETOMAXIMIZED, 0);
        assert_eq!(restored.showCmd, SW_SHOWMINIMIZED as u32);
    }
}
