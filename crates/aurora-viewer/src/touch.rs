//! Touch-friendly UI ergonomics, responsive breakpoints for phones and tablets,
//! and multi-touch world camera gestures (1-finger orbit/tap/long-press,
//! 2-finger pinch zoom and pan).

use std::time::{Duration, Instant};

/// Minimum recommended touch target size in logical density-independent pixels.
#[allow(dead_code)]
pub const MIN_TOUCH_TARGET_DP: f32 = 48.0;
const TAP_MAX_MOVE_DP: f32 = 12.0;
const TAP_MAX_AGE: Duration = Duration::from_millis(350);
const LONG_PRESS_AGE: Duration = Duration::from_millis(500);

/// Responsive layout size class derived from the logical viewport width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormFactor {
    /// Phone portrait (`width < 600 dp`): single-column sheets, no overlapping windows.
    Compact,
    /// Phone landscape or small tablet (`600 dp <= width < 840 dp`).
    Medium,
    /// Large tablet or desktop (`width >= 840 dp`).
    Expanded,
}

impl FormFactor {
    /// Classify the current viewport in logical dp.
    pub fn from_viewport(width_dp: f32, _height_dp: f32) -> Self {
        if width_dp < 600.0 {
            Self::Compact
        } else if width_dp < 840.0 {
            Self::Medium
        } else {
            Self::Expanded
        }
    }

    /// True on touch-primary platforms (Android).
    pub fn is_touch_device() -> bool {
        cfg!(target_os = "android")
    }

    /// Maximum width for floating windows/panels so they never clip off small screens.
    #[allow(dead_code)]
    pub fn panel_max_width(self, viewport_width_dp: f32, desktop_width: f32) -> f32 {
        match self {
            Self::Compact => (viewport_width_dp - 16.0).max(260.0),
            Self::Medium => desktop_width.min((viewport_width_dp - 32.0).max(320.0)),
            Self::Expanded => desktop_width,
        }
    }
}

/// Apply touch-friendly spacing and hit-target minimums on touch devices.
pub fn apply_touch_style(style: &mut egui::Style, form_factor: FormFactor) {
    if !FormFactor::is_touch_device() {
        return;
    }
    let min_h = match form_factor {
        FormFactor::Compact | FormFactor::Medium => 44.0,
        FormFactor::Expanded => 40.0,
    };
    style.spacing.interact_size.y = style.spacing.interact_size.y.max(min_h);
    style.spacing.interact_size.x = style.spacing.interact_size.x.max(min_h);
    style.spacing.button_padding.x = style.spacing.button_padding.x.max(12.0);
    style.spacing.button_padding.y = style.spacing.button_padding.y.max(8.0);
    style.spacing.item_spacing.x = style.spacing.item_spacing.x.max(10.0);
    style.spacing.item_spacing.y = style.spacing.item_spacing.y.max(10.0);
    style.spacing.slider_width = style.spacing.slider_width.max(180.0);
    style.spacing.scroll.bar_width = style.spacing.scroll.bar_width.max(12.0);
}

/// Output of a touch frame / event update for the 3D world camera & interactions.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TouchAction {
    /// 1-finger drag delta (dx, dy) in logical pixels for camera orbit/look.
    pub orbit: [f32; 2],
    /// 2-finger pinch zoom delta (positive = zoom in, negative = zoom out).
    pub zoom: f32,
    /// 2-finger midpoint drag delta (dx, dy) in logical pixels for camera pan.
    pub pan: [f32; 2],
    /// Short 1-finger tap at logical position.
    pub tap: Option<[f32; 2]>,
    /// Long press (hold >= 500 ms without moving) at logical position.
    pub long_press: Option<[f32; 2]>,
}

#[derive(Debug, Clone, Copy)]
struct Finger {
    id: u64,
    start: [f32; 2],
    pos: [f32; 2],
    started_at: Instant,
    moved: bool,
    long_pressed: bool,
}

/// Multi-touch gesture recognizer for 3D world navigation.
#[derive(Default)]
pub struct TouchNav {
    fingers: Vec<Finger>,
}

impl TouchNav {
    /// Process a raw touch event (`Start`, `Move`, `End`, `Cancel`).
    pub fn on_touch(&mut self, phase: winit::event::TouchPhase, id: u64, pos: [f32; 2], now: Instant) -> TouchAction {
        let mut out = TouchAction::default();
        match phase {
            winit::event::TouchPhase::Started => {
                self.fingers.retain(|f| f.id != id);
                self.fingers.push(Finger {
                    id,
                    start: pos,
                    pos,
                    started_at: now,
                    moved: false,
                    long_pressed: false,
                });
            }
            winit::event::TouchPhase::Moved => {
                if self.fingers.len() >= 2 {
                    let prev_mid = midpoint(self.fingers[0].pos, self.fingers[1].pos);
                    let prev_dist = distance(self.fingers[0].pos, self.fingers[1].pos);
                    self.update_finger(id, pos);
                    let new_mid = midpoint(self.fingers[0].pos, self.fingers[1].pos);
                    let new_dist = distance(self.fingers[0].pos, self.fingers[1].pos);
                    out.pan = [new_mid[0] - prev_mid[0], new_mid[1] - prev_mid[1]];
                    out.zoom = (new_dist - prev_dist) * 0.02;
                } else if let Some(f) = self.fingers.iter_mut().find(|f| f.id == id) {
                    let dx = pos[0] - f.pos[0];
                    let dy = pos[1] - f.pos[1];
                    f.pos = pos;
                    if distance(f.start, pos) > TAP_MAX_MOVE_DP {
                        f.moved = true;
                    }
                    if f.moved {
                        out.orbit = [dx, dy];
                    }
                }
            }
            winit::event::TouchPhase::Ended => {
                if let Some(idx) = self.fingers.iter().position(|f| f.id == id) {
                    let f = self.fingers.remove(idx);
                    if self.fingers.is_empty() && !f.moved && !f.long_pressed && now.saturating_duration_since(f.started_at) <= TAP_MAX_AGE
                    {
                        out.tap = Some(pos);
                    }
                }
            }
            winit::event::TouchPhase::Cancelled => {
                self.fingers.retain(|f| f.id != id);
            }
        }
        out
    }

    /// Poll for a long press on an idle stationary finger.
    pub fn poll_long_press(&mut self, now: Instant) -> Option<[f32; 2]> {
        if self.fingers.len() != 1 {
            return None;
        }
        let f = &mut self.fingers[0];
        if !f.moved && !f.long_pressed && now.saturating_duration_since(f.started_at) >= LONG_PRESS_AGE {
            f.long_pressed = true;
            return Some(f.pos);
        }
        None
    }

    /// Clear all active touches (e.g. when UI takes focus or app suspends).
    pub fn clear(&mut self) {
        self.fingers.clear();
    }

    fn update_finger(&mut self, id: u64, pos: [f32; 2]) {
        if let Some(f) = self.fingers.iter_mut().find(|f| f.id == id) {
            f.pos = pos;
            f.moved = true;
        }
    }
}

fn midpoint(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::event::TouchPhase;

    #[test]
    fn form_factor_breakpoints_match_phone_and_tablet_widths() {
        assert_eq!(FormFactor::from_viewport(390.0, 844.0), FormFactor::Compact);
        assert_eq!(FormFactor::from_viewport(720.0, 400.0), FormFactor::Medium);
        assert_eq!(FormFactor::from_viewport(1024.0, 768.0), FormFactor::Expanded);
        assert!(FormFactor::Compact.panel_max_width(360.0, 500.0) <= 344.0);
    }

    #[test]
    fn single_finger_tap_orbit_and_long_press_are_distinct() {
        let mut nav = TouchNav::default();
        let t0 = Instant::now();
        assert_eq!(nav.on_touch(TouchPhase::Started, 1, [100.0, 100.0], t0), TouchAction::default());
        let ended = nav.on_touch(TouchPhase::Ended, 1, [103.0, 102.0], t0 + Duration::from_millis(120));
        assert_eq!(ended.tap, Some([103.0, 102.0]));

        // Long press
        nav.on_touch(TouchPhase::Started, 2, [200.0, 200.0], t0);
        assert_eq!(nav.poll_long_press(t0 + Duration::from_millis(200)), None);
        assert_eq!(nav.poll_long_press(t0 + Duration::from_millis(600)), Some([200.0, 200.0]));
        assert_eq!(nav.poll_long_press(t0 + Duration::from_millis(700)), None);
        let release = nav.on_touch(TouchPhase::Ended, 2, [200.0, 200.0], t0 + Duration::from_millis(750));
        assert_eq!(release.tap, None);

        // Orbit drag
        nav.on_touch(TouchPhase::Started, 3, [50.0, 50.0], t0);
        let drag = nav.on_touch(TouchPhase::Moved, 3, [80.0, 60.0], t0 + Duration::from_millis(40));
        assert_eq!(drag.orbit, [30.0, 10.0]);
    }

    #[test]
    fn two_finger_pinch_and_pan_produce_zoom_and_pan() {
        let mut nav = TouchNav::default();
        let t0 = Instant::now();
        nav.on_touch(TouchPhase::Started, 1, [100.0, 100.0], t0);
        nav.on_touch(TouchPhase::Started, 2, [200.0, 100.0], t0);
        let spread = nav.on_touch(TouchPhase::Moved, 2, [250.0, 100.0], t0 + Duration::from_millis(16));
        assert!(spread.zoom > 0.0);
        assert_eq!(spread.pan, [25.0, 0.0]);
    }
}
