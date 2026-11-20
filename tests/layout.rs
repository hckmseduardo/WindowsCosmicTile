use cosmitile::core::{Axis, Layout, MonitorState, Rect};

#[test]
fn splits_negative_monitor_coordinates_without_overlap() {
    let layout = Layout::split(Axis::Horizontal, 0.5, Layout::Leaf(1), Layout::Leaf(2));
    let frames = layout.frames(Rect::new(-1920, 30, 1920, 1000), 10);
    assert_eq!(frames[&1], Rect::new(-1920, 30, 955, 1000));
    assert_eq!(frames[&2], Rect::new(-955, 30, 955, 1000));
}
#[test]
fn newly_focused_window_is_not_dropped() {
    let mut layout = Layout::Leaf(1);
    layout.insert(2, Some(2), Axis::Horizontal);
    assert_eq!(layout.ids(), vec![1, 2]);
}
#[test]
fn removing_leaf_collapses_its_parent() {
    let tree = Layout::split(Axis::Vertical, 0.5, Layout::Leaf(1), Layout::Leaf(2));
    assert_eq!(tree.remove(1).unwrap().ids(), vec![2]);
}
#[test]
fn resizing_changes_only_requested_axis() {
    let tree = Layout::split(
        Axis::Horizontal,
        0.5,
        Layout::Leaf(1),
        Layout::split(Axis::Vertical, 0.5, Layout::Leaf(2), Layout::Leaf(3)),
    );
    let resized = tree.resized(3, Axis::Horizontal, 0.1);
    let frames = resized.frames(Rect::new(0, 0, 1000, 800), 0);
    assert_eq!(frames[&3].w, 600);
    assert_eq!(frames[&3].h, 400);
}
#[test]
fn monitor_toggles_are_independent() {
    let mut state = MonitorState::default();
    state.toggle("a".into());
    state.toggle("b".into());
    state.toggle("b".into());
    assert!(state.enabled("a"));
    assert!(!state.enabled("b"));
}
#[test]
fn disconnect_does_not_enable_new_screen() {
    let mut state = MonitorState::default();
    state.toggle("a".into());
    state.toggle("b".into());
    state.retain_connected(&["b".into(), "c".into()]);
    assert!(!state.enabled("a"));
    assert!(state.enabled("b"));
    assert!(!state.enabled("c"));
}
#[test]
fn restore_bounds_are_clamped_to_work_area() {
    assert_eq!(
        Rect::new(2000, -400, 3000, 2000).clamped(Rect::new(0, 30, 1920, 1000)),
        Rect::new(0, 30, 1920, 1000)
    );
}
#[test]
fn swap_keeps_tile_geometry() {
    let tree = Layout::split(Axis::Horizontal, 0.3, Layout::Leaf(1), Layout::Leaf(2));
    let area = Rect::new(0, 0, 1000, 800);
    assert_eq!(
        tree.frames(area, 10)[&1],
        tree.swapped(1, 2).frames(area, 10)[&2]
    );
}
#[test]
fn rotation_changes_immediate_split() {
    let tree = Layout::split(Axis::Horizontal, 0.5, Layout::Leaf(1), Layout::Leaf(2));
    let frames = tree.rotated(2).frames(Rect::new(0, 0, 1000, 800), 0);
    assert_eq!(frames[&2], Rect::new(0, 400, 1000, 400));
}
#[test]
fn save_dialog_and_owned_utility_classes_are_excluded() {
    use cosmitile::core::window_eligible;
    assert!(!window_eligible(false, true, false, "#32770"));
    assert!(!window_eligible(false, true, true, "Anything"));
    assert!(!window_eligible(false, false, false, "FixedSavePanel"));
    assert!(window_eligible(false, true, false, "Notepad"));
}
#[test]
fn default_language_is_english_and_unknown_language_falls_back() {
    use cosmitile::i18n::Language;
    assert_eq!(Language::default(), Language::English);
    assert_eq!(Language::from_code("unknown"), Language::English);
    assert_eq!(Language::French.text("Language"), "Langue");
    assert_eq!(Language::Portuguese.text("Language"), "Idioma");
}
#[test]
fn crowded_focused_region_uses_larger_neighbor() {
    use cosmitile::core::choose_target;
    use std::collections::BTreeMap;
    let frames = BTreeMap::from([
        (1, Rect::new(0, 0, 700, 900)),
        (2, Rect::new(700, 0, 700, 445)),
        (3, Rect::new(700, 455, 700, 445)),
    ]);
    assert_eq!(choose_target(&frames, Some(3), 10), Some(1));
}
#[test]
fn direction_chooses_neighbor_in_requested_half_plane() {
    use cosmitile::core::neighbor;
    use std::collections::BTreeMap;
    let frames = BTreeMap::from([
        (1, Rect::new(-1000, 0, 500, 800)),
        (2, Rect::new(-490, 0, 490, 800)),
    ]);
    assert_eq!(neighbor(&frames, 1, 1, 0), Some(2));
    assert_eq!(neighbor(&frames, 1, -1, 0), None);
}
#[test]
fn focus_reaches_windows_on_other_displays_from_an_unmanaged_window() {
    use cosmitile::core::neighbor_from;

    let origin = Rect::new(-800, 0, 800, 600);
    let windows = [
        (1, Rect::new(-1600, 0, 800, 600)),
        (2, Rect::new(40, 30, 800, 600)),
        (3, Rect::new(-800, -650, 800, 600)),
    ];
    assert_eq!(neighbor_from(origin, windows, 1, 0), Some(2));
    assert_eq!(neighbor_from(origin, windows, -1, 0), Some(1));
    assert_eq!(neighbor_from(origin, windows, 0, -1), Some(3));
}

#[test]
fn placement_observation_tolerates_rounding_but_detects_an_ignored_resize() {
    let requested = Rect::new(-1000, 10, 500, 800);
    assert!(requested.matches_placement(Rect::new(-999, 9, 499, 801)));
    assert!(!requested.matches_placement(Rect::new(-1000, 10, 700, 800)));
}
#[test]
fn transfer_waits_for_destination_instead_of_reinserting_on_source() {
    use cosmitile::core::Transfers;
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut transfers = Transfers::default();
    transfers.request(1, "secondary".into(), now);
    assert!(transfers.defer(1, "primary", now + Duration::from_millis(100)));
    assert!(!transfers.defer(1, "secondary", now + Duration::from_millis(200)));
    assert!(!transfers.defer(1, "primary", now + Duration::from_millis(300)));
}
#[test]
fn refused_transfer_expires_without_hiding_window_forever() {
    use cosmitile::core::Transfers;
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut transfers = Transfers::default();
    transfers.request(1, "secondary".into(), now);
    assert!(!transfers.defer(1, "primary", now + Duration::from_secs(3)));
}
