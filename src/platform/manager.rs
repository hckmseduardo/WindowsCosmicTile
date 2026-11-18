use super::{desktop::*, hotkeys, indicator::Indicator};
use cosmitile::{
    core::{choose_target, neighbor, Axis, Layout, MonitorState},
    i18n::Language,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};
use windows_sys::Win32::{
    Foundation::*,
    UI::{Input::KeyboardAndMouse::GetAsyncKeyState, WindowsAndMessaging::*},
};

pub struct Manager {
    pub hwnd: HWND,
    pub monitors: Vec<Monitor>,
    pub policy: MonitorState,
    pub layouts: BTreeMap<String, Layout>,
    pub windows: BTreeMap<isize, Window>,
    pub originals: BTreeMap<isize, Original>,
    pub floating: BTreeSet<isize>,
    pub previous_focus: Option<isize>,
    pub command_target: Option<isize>,
    pub hint: Indicator,
    pub hint_enabled: bool,
    pub resize: bool,
    pub gap: i32,
    pub language: Language,
    pub conflicts: usize,
    pub failures: usize,
}
impl Manager {
    pub unsafe fn new(hwnd: HWND) -> Self {
        let mut result = Self {
            hwnd,
            monitors: monitors(),
            policy: MonitorState::default(),
            layouts: BTreeMap::new(),
            windows: BTreeMap::new(),
            originals: BTreeMap::new(),
            floating: BTreeSet::new(),
            previous_focus: None,
            command_target: None,
            hint: Indicator::new(),
            hint_enabled: true,
            resize: false,
            gap: 10,
            language: Language::English,
            conflicts: 0,
            failures: 0,
        };
        result.load_settings();
        result.conflicts = hotkeys::register(hwnd);
        result
    }
    fn settings_path() -> Option<PathBuf> {
        std::env::var_os("APPDATA").map(|root| {
            PathBuf::from(root)
                .join("WindowsCosmiTile")
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
                    "language={}\ngap={}\nhint={}\ncolor={}\n",
                    self.language.code(),
                    self.gap,
                    self.hint_enabled,
                    self.hint.color
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
        let ids: Vec<_> = self
            .originals
            .iter()
            .filter(|(_, original)| original.monitor == key)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if let Some(original) = self.originals.remove(&id) {
                if visible_frame(id as HWND)
                    .and_then(|r| monitor_for(&self.monitors, r))
                    .as_deref()
                    == Some(key)
                    && IsIconic(id as HWND) == 0
                {
                    restore(id, &original, &self.monitors);
                }
            }
            self.floating.remove(&id);
        }
        self.layouts.remove(key);
    }
    pub unsafe fn restore_all(&mut self) {
        self.set_resize(false);
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
        let current_focus = self.focused_id();
        let windows: Vec<_> = self.windows.values().cloned().collect();
        for window in windows {
            if self.floating.contains(&window.id) {
                continue;
            }
            let Some(key) = monitor_for(&self.monitors, window.frame) else {
                continue;
            };
            if !self.policy.enabled(&key)
                || self
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
            if let Some(tree) = self.layouts.get_mut(&key) {
                let frames = tree.frames(monitor.work.inset(self.gap), self.gap);
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
        if previous != self.layouts || old_monitors != self.monitors {
            self.apply();
        }
        if current_focus.is_some_and(|id| self.windows.contains_key(&id)) {
            self.previous_focus = current_focus;
        }
        self.refresh_hint();
    }
    pub unsafe fn apply(&mut self) {
        self.failures = 0;
        for monitor in &self.monitors {
            if !self.policy.enabled(&monitor.id) {
                continue;
            }
            if let Some(tree) = self.layouts.get(&monitor.id) {
                for (id, frame) in tree.frames(monitor.work.inset(self.gap), self.gap) {
                    if let Some(window) = self.windows.get_mut(&id) {
                        if pid(id as HWND) != window.pid {
                            continue;
                        }
                        if !set_frame(id, frame) {
                            self.failures += 1;
                        }
                        if let Some(actual) = visible_frame(id as HWND) {
                            window.frame = actual;
                        }
                    }
                }
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
    pub unsafe fn direction(&mut self, dx: i32, dy: i32, swap: bool) {
        if self.resize && !swap {
            self.resize_axis(
                if dx == 0 {
                    Axis::Vertical
                } else {
                    Axis::Horizontal
                },
                f64::from(if dx == 0 { dy } else { dx }) * 0.04,
            );
            return;
        }
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
            if swap {
                *tree = tree.swapped(id, other);
                self.apply();
            } else {
                focus(other);
            }
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
        let Some(index) = self.monitors.iter().position(|m| m.id == key) else {
            return;
        };
        let destination = self.monitors[(index + 1) % self.monitors.len()].clone();
        if let Some(tree) = self.layouts.remove(&key).and_then(|tree| tree.remove(id)) {
            self.layouts.insert(key, tree);
        }
        self.floating.remove(&id);
        self.originals.remove(&id);
        if let Some(window) = self.windows.get_mut(&id) {
            let mut frame = window.frame.clamped(destination.work.inset(self.gap));
            frame.x = destination.work.x + self.gap;
            frame.y = destination.work.y + self.gap;
            set_frame(id, frame);
            window.frame = visible_frame(id as HWND).unwrap_or(frame);
        }
        self.reconcile();
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
