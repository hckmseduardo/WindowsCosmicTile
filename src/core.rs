//! Platform-independent layout geometry. All coordinates are desktop pixels.
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(self) -> i32 {
        self.x.saturating_add(self.w)
    }
    pub fn bottom(self) -> i32 {
        self.y.saturating_add(self.h)
    }
    pub fn inset(self, gap: i32) -> Self {
        let gap = gap.max(0).min(self.w.min(self.h).max(0) / 2);
        Self::new(
            self.x + gap,
            self.y + gap,
            (self.w - 2 * gap).max(0),
            (self.h - 2 * gap).max(0),
        )
    }
    pub fn clamped(self, bounds: Self) -> Self {
        let w = self.w.max(1).min(bounds.w.max(1));
        let h = self.h.max(1).min(bounds.h.max(1));
        Self::new(
            self.x.max(bounds.x).min(bounds.right() - w),
            self.y.max(bounds.y).min(bounds.bottom() - h),
            w,
            h,
        )
    }
    pub fn overlap(self, other: Self) -> i64 {
        i64::from((self.right().min(other.right()) - self.x.max(other.x)).max(0))
            * i64::from((self.bottom().min(other.bottom()) - self.y.max(other.y)).max(0))
    }
    pub fn split_axis(self) -> Axis {
        if self.w >= self.h {
            Axis::Horizontal
        } else {
            Axis::Vertical
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Layout {
    Leaf(isize),
    Split(Axis, f64, Box<Layout>, Box<Layout>),
}
impl Layout {
    pub fn split(axis: Axis, ratio: f64, first: Self, second: Self) -> Self {
        Self::Split(
            axis,
            ratio.clamp(0.15, 0.85),
            Box::new(first),
            Box::new(second),
        )
    }
    pub fn ids(&self) -> Vec<isize> {
        match self {
            Self::Leaf(id) => vec![*id],
            Self::Split(_, _, a, b) => [a.ids(), b.ids()].concat(),
        }
    }
    pub fn contains(&self, id: isize) -> bool {
        match self {
            Self::Leaf(value) => *value == id,
            Self::Split(_, _, a, b) => a.contains(id) || b.contains(id),
        }
    }
    pub fn frames(&self, rect: Rect, gap: i32) -> BTreeMap<isize, Rect> {
        match self {
            Self::Leaf(id) => BTreeMap::from([(*id, rect)]),
            Self::Split(axis, ratio, a, b) => {
                let length = if *axis == Axis::Horizontal {
                    rect.w
                } else {
                    rect.h
                }
                .max(0);
                let gap = gap.max(0).min(length);
                let first = ((length - gap) as f64 * ratio.clamp(0.15, 0.85)).round() as i32;
                let mut ra = rect;
                let mut rb = rect;
                match axis {
                    Axis::Horizontal => {
                        ra.w = first;
                        rb.x += first + gap;
                        rb.w = length - first - gap;
                    }
                    Axis::Vertical => {
                        ra.h = first;
                        rb.y += first + gap;
                        rb.h = length - first - gap;
                    }
                }
                let mut frames = a.frames(ra, gap);
                frames.extend(b.frames(rb, gap));
                frames
            }
        }
    }
    pub fn insert(&mut self, id: isize, target: Option<isize>, axis: Axis) {
        if self.contains(id) {
            return;
        }
        let anchor = target
            .filter(|id| self.contains(*id))
            .unwrap_or_else(|| *self.ids().last().unwrap());
        self.insert_known(id, anchor, axis);
    }
    fn insert_known(&mut self, id: isize, target: isize, axis: Axis) {
        match self {
            Self::Leaf(value) if *value == target => {
                *self = Self::split(axis, 0.5, self.clone(), Self::Leaf(id))
            }
            Self::Leaf(_) => (),
            Self::Split(_, _, a, b) => {
                if b.contains(target) {
                    b.insert_known(id, target, axis);
                } else {
                    a.insert_known(id, target, axis);
                }
            }
        }
    }
    pub fn remove(self, id: isize) -> Option<Self> {
        match self {
            Self::Leaf(value) => {
                if value == id {
                    None
                } else {
                    Some(self)
                }
            }
            Self::Split(axis, ratio, a, b) => match (a.remove(id), b.remove(id)) {
                (Some(a), Some(b)) => Some(Self::split(axis, ratio, a, b)),
                (a, b) => a.or(b),
            },
        }
    }
    pub fn swapped(&self, first: isize, second: isize) -> Self {
        match self {
            Self::Leaf(id) => Self::Leaf(if *id == first {
                second
            } else if *id == second {
                first
            } else {
                *id
            }),
            Self::Split(axis, ratio, a, b) => Self::split(
                *axis,
                *ratio,
                a.swapped(first, second),
                b.swapped(first, second),
            ),
        }
    }
    fn has_axis(&self, id: isize, requested: Axis) -> bool {
        match self {
            Self::Leaf(_) => false,
            Self::Split(axis, _, a, b) => {
                self.contains(id)
                    && (*axis == requested
                        || a.has_axis(id, requested)
                        || b.has_axis(id, requested))
            }
        }
    }
    pub fn resized(&self, id: isize, requested: Axis, delta: f64) -> Self {
        match self {
            Self::Leaf(_) => self.clone(),
            Self::Split(axis, ratio, a, b) if self.contains(id) => {
                if a.contains(id) {
                    if a.has_axis(id, requested) {
                        return Self::split(
                            *axis,
                            *ratio,
                            a.resized(id, requested, delta),
                            *b.clone(),
                        );
                    }
                    if *axis == requested {
                        return Self::split(*axis, ratio + delta, *a.clone(), *b.clone());
                    }
                } else {
                    if b.has_axis(id, requested) {
                        return Self::split(
                            *axis,
                            *ratio,
                            *a.clone(),
                            b.resized(id, requested, delta),
                        );
                    }
                    if *axis == requested {
                        return Self::split(*axis, ratio - delta, *a.clone(), *b.clone());
                    }
                }
                self.clone()
            }
            _ => self.clone(),
        }
    }
    pub fn rotated(&self, id: isize) -> Self {
        match self {
            Self::Split(axis, ratio, a, b) => {
                if matches!(**a, Self::Leaf(value) if value == id)
                    || matches!(**b, Self::Leaf(value) if value == id)
                {
                    return Self::split(
                        if *axis == Axis::Horizontal {
                            Axis::Vertical
                        } else {
                            Axis::Horizontal
                        },
                        *ratio,
                        *a.clone(),
                        *b.clone(),
                    );
                }
                Self::split(*axis, *ratio, a.rotated(id), b.rotated(id))
            }
            _ => self.clone(),
        }
    }
}

#[derive(Default)]
pub struct MonitorState {
    pub active: BTreeSet<String>,
}
impl MonitorState {
    pub fn enabled(&self, id: &str) -> bool {
        self.active.contains(id)
    }
    pub fn toggle(&mut self, id: String) {
        if !self.active.insert(id.clone()) {
            self.active.remove(&id);
        }
    }
    pub fn retain_connected(&mut self, connected: &[String]) {
        self.active.retain(|id| connected.contains(id));
    }
    pub fn toggle_all(&mut self, connected: &[String]) {
        if connected.iter().all(|id| self.enabled(id)) {
            self.active.clear();
        } else {
            self.active = connected.iter().cloned().collect();
        }
    }
}

pub fn choose_target(
    frames: &BTreeMap<isize, Rect>,
    preferred: Option<isize>,
    gap: i32,
) -> Option<isize> {
    let fits =
        |r: Rect| ((r.w - gap) / 2 >= 480 && r.h >= 300) || ((r.h - gap) / 2 >= 300 && r.w >= 480);
    if let Some(id) = preferred {
        if frames.get(&id).is_some_and(|r| fits(*r)) {
            return Some(id);
        }
    }
    frames
        .iter()
        .max_by_key(|(id, r)| {
            (
                fits(**r),
                i64::from(r.w) * i64::from(r.h),
                std::cmp::Reverse(**id),
            )
        })
        .map(|(id, _)| *id)
}
pub fn neighbor(frames: &BTreeMap<isize, Rect>, current: isize, dx: i32, dy: i32) -> Option<isize> {
    let origin = frames.get(&current)?;
    frames
        .iter()
        .filter(|(id, _)| **id != current)
        .filter_map(|(id, r)| {
            let x = i64::from(r.x - origin.x) * 2 + i64::from(r.w - origin.w);
            let y = i64::from(r.y - origin.y) * 2 + i64::from(r.h - origin.h);
            let forward = x * i64::from(dx) + y * i64::from(dy);
            if forward <= 0 {
                return None;
            }
            Some((
                *id,
                forward + 3 * (x * i64::from(dy) - y * i64::from(dx)).abs(),
            ))
        })
        .min_by_key(|(_, score)| *score)
        .map(|(id, _)| id)
}

/// Exclude structural dialog/utility types; no localized title matching.
pub fn window_eligible(child: bool, resizable: bool, auxiliary: bool, class_name: &str) -> bool {
    !child
        && resizable
        && !auxiliary
        && class_name != "#32770"
        && class_name != "tooltips_class32"
        && class_name != "Progman"
        && class_name != "WorkerW"
        && class_name != "Shell_TrayWnd"
}
