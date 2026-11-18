use windows_sys::Win32::{Foundation::HWND, UI::Input::KeyboardAndMouse::*};
pub const DIRECTIONS: [(u32, i32, i32); 4] =
    [(0x25, -1, 0), (0x26, 0, -1), (0x27, 1, 0), (0x28, 0, 1)];
pub unsafe fn register(hwnd: HWND) -> usize {
    let mut failures = 0;
    for (id, key, modifiers) in [
        (1, b'T' as u32, MOD_CONTROL | MOD_ALT),
        (2, b'Y' as u32, MOD_CONTROL | MOD_ALT),
        (3, b'T' as u32, MOD_CONTROL | MOD_ALT | MOD_SHIFT),
        (4, b'F' as u32, MOD_CONTROL | MOD_ALT),
        (5, b'G' as u32, MOD_CONTROL | MOD_ALT),
        (6, b'M' as u32, MOD_CONTROL | MOD_ALT),
        (7, b'R' as u32, MOD_CONTROL | MOD_ALT),
        (8, b'O' as u32, MOD_CONTROL | MOD_ALT),
        (9, b'H' as u32, MOD_CONTROL | MOD_ALT),
        (50, 0xBB, MOD_CONTROL | MOD_ALT),
        (51, 0xBD, MOD_CONTROL | MOD_ALT),
        (52, 0xBB, MOD_CONTROL | MOD_ALT | MOD_SHIFT),
        (53, 0xBD, MOD_CONTROL | MOD_ALT | MOD_SHIFT),
    ] {
        if RegisterHotKey(hwnd, id, modifiers | MOD_NOREPEAT, key) == 0 {
            failures += 1;
        }
    }
    for (index, (key, _, _)) in DIRECTIONS.iter().enumerate() {
        for (base, modifiers) in [
            (20, MOD_CONTROL | MOD_ALT),
            (30, MOD_CONTROL | MOD_ALT | MOD_SHIFT),
        ] {
            if RegisterHotKey(hwnd, base + index as i32, modifiers, *key) == 0 {
                failures += 1;
            }
        }
    }
    failures
}
pub unsafe fn resize_mode(hwnd: HWND, enabled: bool) -> usize {
    let mut failures = 0;
    for id in 80..=85 {
        UnregisterHotKey(hwnd, id);
    }
    if enabled {
        for (index, (key, _, _)) in DIRECTIONS.iter().enumerate() {
            if RegisterHotKey(hwnd, 80 + index as i32, 0, *key) == 0 {
                failures += 1;
            }
        }
        for (id, key) in [(84, 0x1B), (85, 0x0D)] {
            if RegisterHotKey(hwnd, id, MOD_NOREPEAT, key) == 0 {
                failures += 1;
            }
        }
    }
    failures
}
pub unsafe fn unregister(hwnd: HWND) {
    for id in 1..=85 {
        UnregisterHotKey(hwnd, id);
    }
}
