use super::desktop::wide;
use cosmitile::core::Rect;
use std::{
    mem::zeroed,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::*},
};

pub struct Indicator {
    pub hwnd: HWND,
    brush: HBRUSH,
    last: Option<(Rect, i32)>,
    pub color: u32,
}
unsafe extern "system" fn procedure(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match message {
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        _ => DefWindowProcW(hwnd, message, w, l),
    }
}
impl Indicator {
    pub unsafe fn new() -> Self {
        let class = wide("WindowsCosmiTile.ActiveHint");
        let instance = GetModuleHandleW(null());
        let color = 0x00E7DC46;
        let brush = CreateSolidBrush(color);
        let definition = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hbrBackground: brush,
            ..zeroed()
        };
        RegisterClassW(&definition);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            class.as_ptr(),
            wide("CosmiTile active hint").as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if !hwnd.is_null() {
            SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
        }
        Self {
            hwnd,
            brush,
            last: None,
            color,
        }
    }
    pub unsafe fn set_color(&mut self, color: u32) {
        if self.color == color || self.hwnd.is_null() {
            return;
        }
        let brush = CreateSolidBrush(color);
        if brush.is_null() {
            return;
        }
        SetClassLongPtrW(self.hwnd, GCLP_HBRBACKGROUND, brush as isize);
        DeleteObject(self.brush);
        self.brush = brush;
        self.color = color;
        InvalidateRect(self.hwnd, null(), 1);
    }
    pub unsafe fn show(&mut self, frame: Rect, target: HWND) {
        if self.hwnd.is_null() || frame.w < 10 || frame.h < 10 {
            self.hide();
            return;
        }
        let width = (3 * GetDpiForWindow(target).max(96) / 96) as i32;
        if self.last == Some((frame, width)) && IsWindowVisible(self.hwnd) != 0 {
            return;
        }
        let outer = CreateRoundRectRgn(0, 0, frame.w + 1, frame.h + 1, 12, 12);
        let inner = CreateRoundRectRgn(width, width, frame.w - width, frame.h - width, 9, 9);
        if !outer.is_null() && !inner.is_null() {
            CombineRgn(outer, outer, inner, RGN_DIFF);
            if SetWindowRgn(self.hwnd, outer, 1) == 0 {
                DeleteObject(outer);
            }
        } else if !outer.is_null() {
            DeleteObject(outer);
        }
        if !inner.is_null() {
            DeleteObject(inner);
        }
        SetWindowPos(
            self.hwnd,
            HWND_TOPMOST,
            frame.x,
            frame.y,
            frame.w,
            frame.h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        self.last = Some((frame, width));
    }
    pub unsafe fn hide(&mut self) {
        ShowWindow(self.hwnd, SW_HIDE);
        self.last = None;
    }
    pub unsafe fn destroy(&mut self) {
        DestroyWindow(self.hwnd);
        UnregisterClassW(
            wide("WindowsCosmiTile.ActiveHint").as_ptr(),
            GetModuleHandleW(null()),
        );
        DeleteObject(self.brush);
        self.hwnd = null_mut();
    }
}
