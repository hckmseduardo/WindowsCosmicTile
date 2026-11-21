//! Win32 UI thread, notification icon, hotkeys and window event delivery.
mod desktop;
mod hotkeys;
mod indicator;
pub(crate) mod installer;
mod keep_awake;
mod manager;

use cosmitile::{core::Axis, i18n::Language};
use desktop::{wide, APP_ICON_RESOURCE};
use manager::Manager;
use std::{
    cell::{Cell, RefCell},
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Accessibility::*, HiDpi::*, Shell::*, WindowsAndMessaging::*},
};

const TRAY: u32 = WM_APP + 1;
const REFRESH: u32 = WM_APP + 2;
thread_local! {
    static STATE: RefCell<Option<Manager>> = const { RefCell::new(None) };
    static OWNER: Cell<HWND> = const { Cell::new(null_mut()) };
    static TICKS: Cell<u32> = const { Cell::new(0) };
}
fn with_state(operation: impl FnOnce(&mut Manager)) {
    STATE.with(|state| {
        if let Ok(mut borrow) = state.try_borrow_mut() {
            if let Some(manager) = borrow.as_mut() {
                operation(manager);
            }
        }
    });
}
unsafe fn tray(manager: &Manager, operation: u32) {
    let mut data: NOTIFYICONDATAW = zeroed();
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = manager.hwnd;
    data.uID = 1;
    data.uCallbackMessage = TRAY;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.hIcon = manager.brand_icon;
    let mode = if manager.resize { " · RESIZE" } else { "" };
    let text = wide(&format!(
        "CosmicTile · {}/{} {}{}",
        manager.policy.active.len(),
        manager.monitors.len(),
        manager.language.text("Display"),
        mode
    ));
    let length = text.len().min(data.szTip.len() - 1);
    data.szTip[..length].copy_from_slice(&text[..length]);
    Shell_NotifyIconW(operation, &data);
}
unsafe fn append(menu: HMENU, id: usize, title: &str, checked: bool) {
    AppendMenuW(
        menu,
        MF_STRING | if checked { MF_CHECKED } else { 0 },
        id,
        wide(title).as_ptr(),
    );
}
unsafe fn show_menu(manager: &mut Manager) {
    let original = GetForegroundWindow();
    manager.command_target = if original.is_null() {
        None
    } else {
        Some(original as isize)
    };
    manager.monitors = desktop::monitors();
    let menu = CreatePopupMenu();
    if menu.is_null() {
        manager.command_target = None;
        return;
    }
    let language = manager.language;
    append(
        menu,
        0,
        concat!("CosmicTile · v", env!("CARGO_PKG_VERSION")),
        false,
    );
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    for (index, monitor) in manager.monitors.iter().enumerate() {
        append(
            menu,
            100 + index,
            &format!(
                "{} {} · {}",
                language.text("Display"),
                index + 1,
                monitor.id
            ),
            manager.policy.enabled(&monitor.id),
        );
    }
    append(
        menu,
        1,
        &format!("{}\tCtrl+Alt+T", language.text("Toggle focused display")),
        false,
    );
    append(
        menu,
        3,
        &format!("{}\tCtrl+Alt+Shift+T", language.text("Toggle all displays")),
        false,
    );
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    for (id, key, shortcut, checked) in [
        (9, "Active window hint", "Ctrl+Alt+H", manager.hint_enabled),
        (4, "Float focused window", "Ctrl+Alt+F", false),
        (7, "Resize mode", "Ctrl+Alt+R", manager.resize),
        (8, "Rotate split", "Ctrl+Alt+O", false),
        (6, "Move to next display", "Ctrl+Alt+M", false),
    ] {
        append(
            menu,
            id,
            &format!("{}\t{}", language.text(key), shortcut),
            checked,
        );
    }
    let colors = CreatePopupMenu();
    for (id, key, color) in [
        (60, "Cyan", 0x00E7DC46),
        (61, "Violet", 0x00FF8FB2),
        (62, "Orange", 0x0059A6FF),
    ] {
        append(colors, id, language.text(key), manager.hint.color == color);
    }
    AppendMenuW(
        menu,
        MF_POPUP,
        colors as usize,
        wide(language.text("Hint color")).as_ptr(),
    );
    let gaps = CreatePopupMenu();
    for (index, value) in [0, 6, 10, 16, 24].iter().enumerate() {
        append(
            gaps,
            70 + index,
            &format!("{} px", value),
            manager.gap == *value,
        );
    }
    AppendMenuW(
        menu,
        MF_POPUP,
        gaps as usize,
        wide(language.text("Gap")).as_ptr(),
    );
    let languages = CreatePopupMenu();
    for (id, text, value) in [
        (90, "English", Language::English),
        (91, "Français", Language::French),
        (92, "Português", Language::Portuguese),
    ] {
        append(languages, id, text, value == language);
    }
    AppendMenuW(
        menu,
        MF_POPUP,
        languages as usize,
        wide(language.text("Language")).as_ptr(),
    );
    append(menu, 10, language.text("Keyboard shortcuts"), false);
    append(menu, 11, language.text("About"), false);
    append(menu, 17, language.text("Official website"), false);
    append(
        menu,
        16,
        language.text("Keep awake"),
        manager.keep_awake.enabled(),
    );
    append(menu, 13, language.text("Check for updates"), false);
    append(menu, 15, language.text("Release notes on GitHub"), false);
    append(
        menu,
        14,
        language.text("Automatically check for updates"),
        manager.auto_updates,
    );
    if manager.conflicts > 0 {
        append(
            menu,
            0,
            &format!(
                "{}: {}",
                language.text("Hotkey conflicts"),
                manager.conflicts
            ),
            false,
        );
    }
    if manager.failures > 0 {
        append(
            menu,
            0,
            &format!(
                "{}: {}",
                language.text("Windows refusing movement"),
                manager.failures
            ),
            false,
        );
    }
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    append(menu, 12, language.text("Quit and restore"), false);
    let mut point: POINT = zeroed();
    GetCursorPos(&mut point);
    // Required notification-area pattern: a foreground owner plus WM_NULL after tracking
    // allows outside clicks to dismiss the context menu reliably.
    SetForegroundWindow(manager.hwnd);
    let selected = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
        point.x,
        point.y,
        0,
        manager.hwnd,
        null(),
    );
    PostMessageW(manager.hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);
    if selected > 0 {
        if matches!(selected, 1 | 4 | 6 | 7 | 8) && !original.is_null() {
            SetForegroundWindow(original);
        }
        command(manager, selected);
    }
    manager.command_target = None;
}
unsafe fn help(manager: &Manager) {
    let language = manager.language;
    let rows = [
        ("Ctrl+Alt+T / Y", "Toggle focused display"),
        ("Ctrl+Alt+Shift+T", "Toggle all displays"),
        ("Ctrl+Alt+F / G", "Float focused window"),
        ("Ctrl+Alt+← ↑ ↓ →", "Focus window"),
        ("Ctrl+Alt+Shift+← ↑ ↓ →", "Swap windows"),
        ("Ctrl+Alt+R", "Resize mode"),
        ("← / → (R)", "Decrease/increase width"),
        ("↑ / ↓ (R)", "Decrease/increase height"),
        ("Escape / Enter", "Exit resize mode"),
        ("Ctrl+Alt+O", "Rotate split"),
        ("Ctrl+Alt+M", "Move to next display"),
        ("Ctrl+Alt+H", "Active window hint"),
    ];
    let text = rows
        .iter()
        .map(|(keys, title)| format!("{} — {}", keys, language.text(title)))
        .collect::<Vec<_>>()
        .join("\n");
    MessageBoxW(
        manager.hwnd,
        wide(&text).as_ptr(),
        wide(language.text("Keyboard shortcuts")).as_ptr(),
        MB_OK | MB_ICONINFORMATION,
    );
}
unsafe fn open_web_page(owner: HWND, url: &str) {
    ShellExecuteW(
        owner,
        wide("open").as_ptr(),
        wide(url).as_ptr(),
        null(),
        null(),
        SW_SHOWNORMAL,
    );
}
unsafe fn command(manager: &mut Manager, id: i32) {
    match id {
        1 | 2 => {
            if let Some(key) = manager.focused_monitor() {
                manager.toggle_monitor(key);
            }
        }
        3 => manager.toggle_all(),
        4 | 5 => manager.float_focused(),
        6 => manager.next_monitor(),
        7 => manager.toggle_resize(),
        8 => manager.rotate(),
        9 => {
            manager.hint_enabled = !manager.hint_enabled;
            manager.refresh_hint();
        }
        10 => help(manager),
        11 => {
            MessageBoxW(manager.hwnd, wide(&format!("CosmicTile v{}\n{}\nhttps://github.com/hckmseduardo/WindowsCosmicTile", env!("CARGO_PKG_VERSION"), manager.language.text("Native Rust + Win32 preview. Starts with tiling off. Save/Open dialogs stay floating."))).as_ptr(), wide("WindowsCosmicTile").as_ptr(), MB_OK);
        }
        16 => {
            if manager.keep_awake.toggle().is_err() {
                MessageBoxW(
                    manager.hwnd,
                    wide(manager.language.text("Could not change keep awake state.")).as_ptr(),
                    wide("CosmicTile").as_ptr(),
                    MB_OK | MB_ICONERROR,
                );
            }
        }
        13 => {
            start_update_check(manager, true);
        }
        14 => {
            manager.auto_updates = !manager.auto_updates;
        }
        15 => {
            open_web_page(
                manager.hwnd,
                "https://github.com/hckmseduardo/WindowsCosmicTile/releases",
            );
        }
        17 => open_web_page(manager.hwnd, "https://cosmictile.app.amazing-ai.tools"),
        12 => {
            PostMessageW(manager.hwnd, WM_CLOSE, 0, 0);
        }
        20..=23 => {
            let (_, x, y) = hotkeys::DIRECTIONS[(id - 20) as usize];
            manager.focus_direction(x, y);
        }
        30..=33 => {
            let (_, x, y) = hotkeys::DIRECTIONS[(id - 30) as usize];
            manager.swap_direction(x, y);
        }
        50 => manager.resize_axis(Axis::Horizontal, 0.04),
        51 => manager.resize_axis(Axis::Horizontal, -0.04),
        52 => manager.resize_axis(Axis::Vertical, 0.04),
        53 => manager.resize_axis(Axis::Vertical, -0.04),
        60..=62 => manager
            .hint
            .set_color([0x00E7DC46, 0x00FF8FB2, 0x0059A6FF][(id - 60) as usize]),
        70..=74 => {
            manager.gap = [0, 6, 10, 16, 24][(id - 70) as usize];
            manager.apply();
        }
        80..=83 if manager.resize => {
            let (_, x, y) = hotkeys::DIRECTIONS[(id - 80) as usize];
            let (axis, direction) = if x == 0 {
                (Axis::Vertical, y)
            } else {
                (Axis::Horizontal, x)
            };
            manager.resize_axis(axis, f64::from(direction) * 0.04);
        }
        84 | 85 => manager.set_resize(false),
        90..=92 => {
            manager.language =
                [Language::English, Language::French, Language::Portuguese][(id - 90) as usize]
        }
        100..=199 => {
            if let Some(monitor) = manager.monitors.get((id - 100) as usize) {
                manager.toggle_monitor(monitor.id.clone());
            }
        }
        _ => (),
    }
    manager.save_settings();
    tray(manager, NIM_MODIFY);
}
fn start_update_check(manager: &mut Manager, manual: bool) {
    if manager.updater.check(manual) {
        manager.last_update_check = cosmitile::updates::now();
        manager.save_settings();
    } else if manual {
        unsafe {
            update_message(
                manager,
                "An update operation is already in progress.",
                MB_ICONINFORMATION,
            );
        }
    }
}
unsafe fn update_message(manager: &mut Manager, key: &'static str, icon: u32) {
    manager.set_resize(false);
    MessageBoxW(
        manager.hwnd,
        wide(manager.language.text(key)).as_ptr(),
        wide("CosmicTile Updates").as_ptr(),
        MB_OK | icon,
    );
}
unsafe fn poll_updates(manager: &mut Manager) {
    use cosmitile::updates::Event;
    while let Some(event) = manager.updater.poll() {
        match event {
            Event::Checked {
                manual,
                result: Ok(Some(release)),
            } => {
                let version = release.version.to_string();
                if !manual && (!manager.auto_updates || manager.declined_update == version) {
                    manager.updater.consent(None);
                    continue;
                }
                let text = format!(
                    "CosmicTile v{version}\n\n{}\n\n{}",
                    cosmitile::updates::note_summary(&release.notes),
                    manager.language.text("Install this update now? CosmicTile will restart when the update is ready."),
                );
                manager.set_resize(false);
                let approved = MessageBoxW(
                    manager.hwnd,
                    wide(&text).as_ptr(),
                    wide(manager.language.text("Update available")).as_ptr(),
                    MB_YESNO | MB_DEFBUTTON2 | MB_ICONQUESTION,
                ) == IDYES;
                if approved && !installer::can_install() {
                    manager.updater.consent(None);
                    update_message(manager, "Open CosmicTile without administrator permissions to install updates.", MB_ICONINFORMATION);
                } else if approved {
                    match std::env::current_exe() {
                        Ok(executable) => manager.updater.consent(Some(executable)),
                        Err(_) => {
                            manager.updater.consent(None);
                            update_message(manager, "Could not locate the installed executable.", MB_ICONERROR);
                        }
                    }
                } else {
                    manager.declined_update = version;
                    manager.save_settings();
                    manager.updater.consent(None);
                }
            }
            Event::Checked { manual: true, result: Ok(None) } => update_message(manager, "No newer compatible release is available.", MB_ICONINFORMATION),
            Event::Checked { manual: true, result: Err(_) } => update_message(manager, "Could not check for updates. Please try again later.", MB_ICONERROR),
            Event::Checked { .. } => (),
            Event::Downloaded(Ok(update)) => {
                if installer::start(update, manager.language).is_ok() {
                    PostMessageW(manager.hwnd, WM_CLOSE, 0, 0);
                } else {
                    update_message(manager, "Could not install the update. Please try again.", MB_ICONERROR);
                }
            }
            Event::Downloaded(Err(_)) => update_message(manager, "Could not prepare the update. Check your connection and write access to the app folder, then retry.", MB_ICONERROR),
        }
    }
}
unsafe extern "system" fn event_hook(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    OWNER.with(|owner| {
        if !owner.get().is_null() {
            PostMessageW(owner.get(), REFRESH, 0, 0);
        }
    });
}
unsafe extern "system" fn procedure(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match message {
        WM_TIMER => {
            TICKS.with(|tick| {
                let next = tick.get().wrapping_add(1);
                tick.set(next);
                with_state(|s| {
                    poll_updates(s);
                    if cosmitile::updates::due(
                        s.auto_updates,
                        s.last_update_check,
                        cosmitile::updates::now(),
                    ) {
                        start_update_check(s, false);
                    }
                    if next % 3 == 0 {
                        s.reconcile();
                    } else {
                        s.refresh_hint();
                    }
                });
            });
            0
        }
        WM_HOTKEY => {
            with_state(|s| command(s, w as i32));
            0
        }
        TRAY => {
            if matches!(l as u32, WM_LBUTTONUP | WM_RBUTTONUP | WM_CONTEXTMENU) {
                with_state(|s| show_menu(s));
            }
            0
        }
        REFRESH | WM_DISPLAYCHANGE => {
            with_state(|s| {
                s.reconcile();
                tray(s, NIM_MODIFY);
            });
            0
        }
        WM_QUERYENDSESSION => 1,
        WM_ENDSESSION => {
            if w != 0 {
                with_state(|s| {
                    s.keep_awake.disable();
                    s.restore_all();
                });
            }
            0
        }
        WM_CLOSE => {
            with_state(|s| {
                s.keep_awake.disable();
                s.restore_all();
            });
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => {
            if message == RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) {
                with_state(|s| tray(s, NIM_ADD));
                return 0;
            }
            DefWindowProcW(hwnd, message, w, l)
        }
    }
}
pub fn run(restart: Option<installer::Restart>) {
    let timeout = if restart.is_some() {
        installer::HANDOFF_TIMEOUT_MS
    } else {
        0
    };
    let Ok(_instance_guard) = installer::Instance::acquire(timeout) else {
        return;
    };
    // Win32 requires one owning UI thread. Every raw handle remains on this thread.
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(null());
        let class = wide("WindowsCosmicTile.Controller");
        let definition = WNDCLASSW {
            hIcon: LoadIconW(instance, APP_ICON_RESOURCE),
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        RegisterClassW(&definition);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide("WindowsCosmicTile").as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        OWNER.with(|owner| owner.set(hwnd));
        STATE.with(|state| *state.borrow_mut() = Some(Manager::new(hwnd)));
        with_state(|s| {
            s.reconcile();
            tray(s, NIM_ADD);
        });
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            null_mut(),
            Some(event_hook),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        SetTimer(hwnd, 1, 120, None);
        if let Some(restart) = restart {
            let language = STATE.with(|state| {
                state
                    .borrow()
                    .as_ref()
                    .map_or(Language::English, |s| s.language)
            });
            installer::finish_restart(restart, language);
        }
        let mut message: MSG = zeroed();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        KillTimer(hwnd, 1);
        if !hook.is_null() {
            UnhookWinEvent(hook);
        }
        with_state(|s| {
            tray(s, NIM_DELETE);
            s.destroy();
        });
        STATE.with(|state| {
            state.borrow_mut().take();
        });
        OWNER.with(|owner| owner.set(null_mut()));
    }
}

#[cfg(test)]
mod brand_tests {
    use super::*;
    #[test]
    fn shared_icon_is_embedded_in_the_native_executable() {
        unsafe {
            let instance = GetModuleHandleW(null());
            for size in [16, 32, 48, 256] {
                let icon = LoadImageW(
                    instance,
                    APP_ICON_RESOURCE,
                    IMAGE_ICON,
                    size,
                    size,
                    LR_SHARED,
                );
                assert!(!icon.is_null(), "Missing CosmicTile icon at {size}px");
            }
        }
    }
}
