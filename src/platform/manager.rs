use super::{desktop::*, hotkeys, indicator::Indicator};
use cosmitile::{
    core::{
        choose_target, neighbor, neighbor_from, Axis, Layout, MonitorState, Rect, Size, Transfers,
    },
    i18n::Language,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    UI::{
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{GetAsyncKeyState, IsWindowEnabled},
        WindowsAndMessaging::*,
    },
};

const POSITION_TIMEOUT: Duration = Duration::from_secs(2);
const POSITION_SETTLE_TIME: Duration = Duration::from_millis(300);

struct PendingPosition {
    frame: Rect,
    deadline: Instant,
    pid: u32,
    // None while waiting for a maximized window to restore.
    check_after: Option<Instant>,
    before_correction: Option<Rect>,
}
struct WindowMinimum {
    pid: u32,
    dpi: u32,
    monitor: String,
    size: Size,
}
pub struct Manager {
    pub keep_awake: super::keep_awake::KeepAwake,
    pub hwnd: HWND,
    pub brand_icon: HICON,
    pub monitors: Vec<Monitor>,
    pub policy: MonitorState,
    pub layouts: BTreeMap<String, Layout>,
    pub windows: BTreeMap<isize, Window>,
    pub originals: BTreeMap<isize, Original>,
    pub floating: BTreeSet<isize>,
    pub pending_transfers: Transfers,
    pending_positions: BTreeMap<isize, PendingPosition>,
    minimum_sizes: BTreeMap<isize, WindowMinimum>,
    pub previous_focus: Option<isize>,
    pub command_target: Option<isize>,
    pub hint: Indicator,
    pub hint_enabled: bool,
    pub resize: bool,
    pub gap: i32,
    pub language: Language,
    pub conflicts: usize,
    pub failures: usize,
    pub updater: cosmitile::updates::Updater,
    pub auto_updates: bool,
    pub last_update_check: u64,
    pub declined_update: String,
}
impl Manager {
    pub unsafe fn new(hwnd: HWND) -> Self {
        let instance =
            windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
        let brand_icon = LoadImageW(
            instance,
            APP_ICON_RESOURCE,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            LR_SHARED,
        ) as HICON;
        let mut result = Self {
            keep_awake: super::keep_awake::KeepAwake::default(),
            hwnd,
            brand_icon,
            monitors: monitors(),
            policy: MonitorState::default(),
            layouts: BTreeMap::new(),
            windows: BTreeMap::new(),
            originals: BTreeMap::new(),
            floating: BTreeSet::new(),
            pending_transfers: Transfers::default(),
            pending_positions: BTreeMap::new(),
            minimum_sizes: BTreeMap::new(),
            previous_focus: None,
            command_target: None,
            hint: Indicator::new(),
            hint_enabled: true,
            resize: false,
            gap: 10,
            language: Language::English,
            conflicts: 0,
            failures: 0,
            updater: cosmitile::updates::Updater::default(),
            auto_updates: true,
            last_update_check: 0,
            declined_update: String::new(),
        };
        result.load_settings();
        result.conflicts = hotkeys::register(hwnd);
        result
    }
    fn settings_path() -> Option<PathBuf> {
        std::env::var_os("APPDATA").map(|root| {
            PathBuf::from(root)
                .join("WindowsCosmicTile")
                .join("settings.conf")
        })
    }
    fn load_settings(&mut self) {
        let content = Self::settings_path()
            .and_then(|path| fs::read_to_string(path).ok())
            .unwrap_or_default();
        for line in content.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "auto_updates" => self.auto_updates = value != "false",
                    "last_update_check" => self.last_update_check = value.parse().unwrap_or(0),
                    "declined_update" => self.declined_update = value.to_string(),
                    "language" => self.language = Language::from_code(value),
                    "gap" => self.gap = value.parse::<i32>().unwrap_or(10).clamp(0, 40),
                    "hint" => self.hint_enabled = value != "false",
                    "color" => {
                        if let Ok(color) = value.parse::<u32>() {
                            unsafe {
                                self.hint.set_color(color);
                            }
                        }
                    }
                    _ => (),
                }
            }
        }
    }
    pub fn save_settings(&self) {
        if let Some(path) = Self::settings_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(
                path,
                format!(
                    "language={}\ngap={}\nhint={}\ncolor={}\nauto_updates={}\nlast_update_check={}\ndeclined_update={}\n",
                    self.language.code(),
                    self.gap,
                    self.hint_enabled,
                    self.hint.color,
                    self.auto_updates, self.last_update_check, self.declined_update
                ),
            );
        }
    }
    pub unsafe fn focused_id(&self) -> Option<isize> {
        self.command_target.or_else(|| {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                None
            } else {
                Some(hwnd as isize)
            }
        })
    }
    pub unsafe fn focused_monitor(&self) -> Option<String> {
        if let Some(rect) = self.focused_id().and_then(|id| visible_frame(id as HWND)) {
            return monitor_for(&self.monitors, rect);
        }
        let mut point: POINT = std::mem::zeroed();
        GetCursorPos(&mut point);
        self.monitors
            .iter()
            .find(|monitor| {
                point.x >= monitor.work.x
                    && point.x < monitor.work.right()
                    && point.y >= monitor.work.y
                    && point.y < monitor.work.bottom()
            })
            .map(|m| m.id.clone())
            .or_else(|| self.monitors.first().map(|m| m.id.clone()))
    }
    pub unsafe fn toggle_monitor(&mut self, key: String) {
        if !self.monitors.iter().any(|monitor| monitor.id == key) {
            return;
        }
        let previous = self.policy.enabled(&key);
        self.policy.toggle(key.clone());
        if previous {
            self.release_monitor(&key);
        }
        self.set_resize(false);
        self.reconcile();
        self.refresh_hint();
    }
    pub unsafe fn toggle_all(&mut self) {
        let connected: Vec<_> = self.monitors.iter().map(|m| m.id.clone()).collect();
        if connected.iter().all(|key| self.policy.enabled(key)) {
            self.restore_all();
        } else {
            self.policy.toggle_all(&connected);
            self.reconcile();
        }
    }
    pub unsafe fn release_monitor(&mut self, key: &str) {
        self.minimum_sizes
            .retain(|_, minimum| minimum.monitor != key);
        let ids: Vec<_> = self
            .originals
            .iter()
            .filter(|(_, original)| original.monitor == key)
            .map(|(id, _)| *id)
            .collect();
        if let Some(tree) = self.layouts.remove(key) {
            for id in tree.ids() {
                self.pending_positions.remove(&id);
            }
        }
        for id in ids {
            self.pending_positions.remove(&id);
            if let Some(original) = self.originals.remove(&id) {
                if monitor_id(id as HWND).as_deref() == Some(key) {
                    restore(id, &original, &self.monitors);
                }
            }
            self.floating.remove(&id);
        }
    }
    pub unsafe fn restore_all(&mut self) {
        self.set_resize(false);
        self.pending_positions.clear();
        self.minimum_sizes.clear();
        self.pending_transfers = Transfers::default();
        let owners: BTreeSet<_> = self
            .originals
            .values()
            .map(|original| original.monitor.clone())
            .collect();
        for key in owners {
            self.release_monitor(&key);
        }
        self.layouts.clear();
        self.policy.active.clear();
        self.hint.hide();
    }
    pub unsafe fn reconcile(&mut self) {
        if GetAsyncKeyState(1) < 0 {
            return;
        }
        let mut reflow = self.retry_positions();
        let old_monitors = self.monitors.clone();
        self.monitors = monitors();
        self.policy.retain_connected(
            &self
                .monitors
                .iter()
                .map(|m| m.id.clone())
                .collect::<Vec<_>>(),
        );
        let previous = self.layouts.clone();
        self.windows = windows().into_iter().map(|w| (w.id, w)).collect();
        let minimum_count = self.minimum_sizes.len();
        self.minimum_sizes.retain(|id, minimum| {
            self.windows.get(id).is_some_and(|window| {
                window.pid == minimum.pid
                    && GetDpiForWindow(*id as HWND) == minimum.dpi
                    && monitor_for(&self.monitors, window.frame).as_deref()
                        == Some(&minimum.monitor)
            })
        });
        reflow |= self.minimum_sizes.len() != minimum_count;
        let keys: Vec<_> = self.layouts.keys().cloned().collect();
        for key in keys {
            if !self.policy.enabled(&key) {
                self.layouts.remove(&key);
                continue;
            }
            let ids = self.layouts[&key].ids();
            for id in ids {
                let remove = self.floating.contains(&id)
                    || self
                        .windows
                        .get(&id)
                        .and_then(|w| monitor_for(&self.monitors, w.frame))
                        .as_deref()
                        != Some(key.as_str());
                if remove {
                    if let Some(tree) = self.layouts.remove(&key).and_then(|tree| tree.remove(id)) {
                        self.layouts.insert(key.clone(), tree);
                    }
                }
            }
        }
        // PID validation avoids restoring a reused HWND belonging to another process.
        self.originals
            .retain(|id, original| IsWindow(*id as HWND) != 0 && pid(*id as HWND) == original.pid);
        self.floating.retain(|id| self.originals.contains_key(id));
        let now = std::time::Instant::now();
        self.pending_transfers.expire(now);
        let current_focus = self.focused_id();
        let windows: Vec<_> = self.windows.values().cloned().collect();
        for window in windows {
            if self.floating.contains(&window.id) {
                continue;
            }
            let Some(key) = monitor_for(&self.monitors, window.frame) else {
                continue;
            };
            if self.pending_transfers.defer(window.id, &key, now) {
                continue;
            }
            if !self.policy.enabled(&key) {
                if self
                    .originals
                    .get(&window.id)
                    .is_some_and(|original| original.monitor != key)
                {
                    self.originals.remove(&window.id);
                }
                continue;
            }
            if self
                .layouts
                .get(&key)
                .is_some_and(|tree| tree.contains(window.id))
            {
                continue;
            }
            let Some(monitor) = self.monitors.iter().find(|m| m.id == key) else {
                continue;
            };
            if self
                .originals
                .get(&window.id)
                .map(|original| &original.monitor)
                != Some(&key)
            {
                if let Some(original) = snapshot(&window, key.clone()) {
                    self.originals.insert(window.id, original);
                }
            }
            let minimums = self.monitor_minimums(&key);
            if let Some(tree) = self.layouts.get_mut(&key) {
                let frames =
                    tree.frames_with_minimums(monitor.work.inset(self.gap), self.gap, &minimums);
                let preferred = current_focus
                    .filter(|id| tree.contains(*id))
                    .or(self.previous_focus.filter(|id| tree.contains(*id)));
                let target = choose_target(&frames, preferred, self.gap);
                let axis = target
                    .and_then(|id| frames.get(&id))
                    .copied()
                    .unwrap_or(monitor.work)
                    .split_axis();
                tree.insert(window.id, target, axis);
            } else {
                self.layouts.insert(key, Layout::Leaf(window.id));
            }
        }
        if reflow || previous != self.layouts || old_monitors != self.monitors {
            self.apply();
        }
        if current_focus.is_some_and(|id| self.windows.contains_key(&id)) {
            self.previous_focus = current_focus;
        }
        self.refresh_hint();
    }
    unsafe fn queue_position(&mut self, id: isize, frame: Rect) -> bool {
        let Some(window) = self.windows.get(&id) else {
            return false;
        };
        let process = window.pid;
        let now = Instant::now();
        if let Some(pending) = self.pending_positions.get_mut(&id) {
            if pending.pid == process && now < pending.deadline {
                if pending.check_after.is_none() {
                    pending.frame = frame;
                    return true;
                }
                if pending.frame == frame {
                    return true;
                }
            }
        }
        let check_after = match set_frame(id, frame) {
            PositionStatus::Queued => Some(now + POSITION_SETTLE_TIME),
            PositionStatus::Restoring => None,
            PositionStatus::Failed => {
                self.pending_positions.remove(&id);
                return false;
            }
        };
        self.pending_positions.insert(
            id,
            PendingPosition {
                frame,
                deadline: now + POSITION_TIMEOUT,
                pid: process,
                check_after,
                before_correction: None,
            },
        );
        true
    }
    unsafe fn retry_positions(&mut self) -> bool {
        let now = Instant::now();
        let mut reflow = false;
        self.pending_positions.retain(|id, request| {
            let hwnd = *id as HWND;
            if pid(hwnd) != request.pid
                || IsWindowVisible(hwnd) == 0
                || IsIconic(hwnd) != 0
                || is_cloaked(hwnd)
            {
                return false;
            }
            if now >= request.deadline {
                self.failures += 1;
                return false;
            }
            if IsZoomed(hwnd) != 0 {
                return true;
            }
            if let Some(check_after) = request.check_after {
                if now < check_after {
                    return true;
                }
                let Some(actual) = visible_frame(hwnd) else {
                    return false;
                };
                let Some(monitor) = monitor_for(&self.monitors, actual) else {
                    return false;
                };
                if request.frame.matches_placement(actual)
                    || monitor_for(&self.monitors, request.frame).as_deref() != Some(&monitor)
                {
                    return false;
                }
                if let Some(before) = request.before_correction {
                    let observed = request.frame.observed_minimum(before, actual);
                    // Learn only from tiles, since a monitor transfer also
                    // uses the placement queue with an intermediate frame.
                    if observed != Size::default()
                        && self
                            .layouts
                            .get(&monitor)
                            .is_some_and(|tree| tree.contains(*id))
                    {
                        let dpi = GetDpiForWindow(hwnd);
                        let previous = self
                            .minimum_sizes
                            .get(id)
                            .filter(|m| {
                                m.pid == request.pid && m.dpi == dpi && m.monitor == monitor
                            })
                            .map(|m| m.size)
                            .unwrap_or_default();
                        // Learn once per axis so the layout cannot grow repeatedly
                        // when an app adds padding to every request.
                        let size = Size {
                            w: if previous.w == 0 {
                                observed.w
                            } else {
                                previous.w
                            },
                            h: if previous.h == 0 {
                                observed.h
                            } else {
                                previous.h
                            },
                        };
                        if size != previous {
                            self.minimum_sizes.insert(
                                *id,
                                WindowMinimum {
                                    pid: request.pid,
                                    dpi,
                                    monitor,
                                    size,
                                },
                            );
                            reflow = true;
                            return false;
                        }
                    }
                    self.failures += 1;
                    return false;
                }
                // Recalculate invisible borders after the first placement settles.
                match set_frame(*id, request.frame) {
                    PositionStatus::Queued => {
                        request.before_correction = Some(actual);
                        request.check_after = Some(now + POSITION_SETTLE_TIME);
                        request.deadline = now + POSITION_TIMEOUT;
                        return true;
                    }
                    PositionStatus::Restoring => {
                        request.check_after = None;
                        return true;
                    }
                    PositionStatus::Failed => self.failures += 1,
                }
                return false;
            }
            match set_frame(*id, request.frame) {
                PositionStatus::Queued => {
                    request.check_after = Some(now + POSITION_SETTLE_TIME);
                    request.deadline = now + POSITION_TIMEOUT;
                    true
                }
                PositionStatus::Failed => {
                    self.failures += 1;
                    false
                }
                PositionStatus::Restoring => true,
            }
        });
        reflow
    }
    fn monitor_minimums(&self, monitor: &str) -> BTreeMap<isize, Size> {
        self.minimum_sizes
            .iter()
            .filter(|(_, minimum)| minimum.monitor == monitor)
            .map(|(id, minimum)| (*id, minimum.size))
            .collect()
    }
    pub unsafe fn apply(&mut self) {
        self.failures = 0;
        let mut requests = Vec::new();
        for monitor in &self.monitors {
            if !self.policy.enabled(&monitor.id) {
                continue;
            }
            if let Some(tree) = self.layouts.get(&monitor.id) {
                requests.extend(tree.frames_with_minimums(
                    monitor.work.inset(self.gap),
                    self.gap,
                    &self.monitor_minimums(&monitor.id),
                ));
            }
        }
        for (id, frame) in requests {
            if self
                .windows
                .get(&id)
                .is_some_and(|window| pid(id as HWND) == window.pid)
                && !self.queue_position(id, frame)
            {
                self.failures += 1;
            }
        }
        self.refresh_hint();
    }
    pub unsafe fn current(&self) -> Option<(isize, String)> {
        let id = self.focused_id()?;
        let window = self.windows.get(&id)?;
        Some((id, monitor_for(&self.monitors, window.frame)?))
    }
    pub unsafe fn refresh_hint(&mut self) {
        let tiled_focus = self
            .current()
            .is_some_and(|(id, key)| self.layouts.get(&key).is_some_and(|tree| tree.contains(id)));
        if self.resize && !tiled_focus {
            self.set_resize(false);
        }
        if self.hint_enabled {
            if let Some((id, key)) = self.current() {
                if self.policy.enabled(&key)
                    && (self.layouts.get(&key).is_some_and(|tree| tree.contains(id))
                        || self.floating.contains(&id))
                {
                    if let Some(frame) = visible_frame(id as HWND) {
                        self.hint.show(frame, id as HWND);
                        return;
                    }
                }
            }
        }
        self.hint.hide();
    }
    pub unsafe fn set_resize(&mut self, active: bool) {
        self.resize = active;
        self.conflicts += hotkeys::resize_mode(self.hwnd, active);
    }
    pub unsafe fn toggle_resize(&mut self) {
        if self.resize {
            self.set_resize(false);
            return;
        }
        if let Some((id, key)) = self.current() {
            if self.layouts.get(&key).is_some_and(|tree| tree.contains(id)) {
                self.set_resize(true);
            }
        }
    }
    pub unsafe fn focus_direction(&self, dx: i32, dy: i32) {
        let Some(id) = self.focused_id() else {
            return;
        };
        let Some(origin) = visible_frame(id as HWND) else {
            return;
        };
        // Use live desktop geometry, including floating windows and displays
        // where tiling is disabled. Focusing never changes a layout tree.
        let candidates = windows().into_iter().filter_map(|window| {
            (window.id != id
                && IsWindowEnabled(window.id as HWND) != 0
                && monitor_id(window.id as HWND).is_some())
            .then_some((window.id, window.frame))
        });
        if let Some(other) = neighbor_from(origin, candidates, dx, dy) {
            focus(other);
        }
    }
    pub unsafe fn swap_direction(&mut self, dx: i32, dy: i32) {
        let Some((id, key)) = self.current() else {
            return;
        };
        let Some(tree) = self.layouts.get_mut(&key) else {
            return;
        };
        let frames = tree
            .ids()
            .into_iter()
            .filter_map(|id| self.windows.get(&id).map(|window| (id, window.frame)))
            .collect();
        if let Some(other) = neighbor(&frames, id, dx, dy) {
            *tree = tree.swapped(id, other);
            self.apply();
        }
    }
    pub unsafe fn resize_axis(&mut self, axis: Axis, delta: f64) {
        if let Some((id, key)) = self.current() {
            if let Some(tree) = self.layouts.get_mut(&key) {
                *tree = tree.resized(id, axis, delta);
                self.apply();
            }
        }
    }
    pub unsafe fn rotate(&mut self) {
        if let Some((id, key)) = self.current() {
            if let Some(tree) = self.layouts.get_mut(&key) {
                *tree = tree.rotated(id);
                self.apply();
            }
        }
    }
    pub unsafe fn float_focused(&mut self) {
        let Some((id, key)) = self.current() else {
            return;
        };
        if !self.policy.enabled(&key) {
            return;
        }
        if !self.floating.remove(&id) {
            if !self.layouts.get(&key).is_some_and(|tree| tree.contains(id)) {
                return;
            }
            self.pending_positions.remove(&id);
            self.floating.insert(id);
            if let Some(tree) = self.layouts.remove(&key).and_then(|tree| tree.remove(id)) {
                self.layouts.insert(key, tree);
            }
            if let Some(original) = self.originals.get(&id) {
                restore(id, original, &self.monitors);
            }
        }
        self.reconcile();
        self.apply();
    }
    pub unsafe fn next_monitor(&mut self) {
        let Some((id, key)) = self.current() else {
            return;
        };
        if self.monitors.len() < 2 {
            return;
        }
        let Some(index) = self.monitors.iter().position(|monitor| monitor.id == key) else {
            return;
        };
        let destination = self.monitors[(index + 1) % self.monitors.len()].clone();
        let Some(window) = self.windows.get(&id) else {
            return;
        };
        let mut frame = window.frame.clamped(destination.work.inset(self.gap));
        frame.x = destination.work.x + self.gap;
        frame.y = destination.work.y + self.gap;
        if !self.queue_position(id, frame) {
            self.failures += 1;
            return;
        }
        if let Some(tree) = self.layouts.remove(&key).and_then(|tree| tree.remove(id)) {
            self.layouts.insert(key, tree);
        }
        self.floating.remove(&id);
        self.pending_transfers
            .request(id, destination.id, std::time::Instant::now());
        // Do not rescan immediately: SWP_ASYNCWINDOWPOS queues the move on the
        // target thread. Rejoin only after observing its destination (or timeout).
        self.apply();
        focus(id);
    }
    pub unsafe fn destroy(&mut self) {
        self.restore_all();
        hotkeys::unregister(self.hwnd);
        self.hint.destroy();
        self.save_settings();
    }
}
