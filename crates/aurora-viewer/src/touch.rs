//! Touch-friendly UI ergonomics, responsive breakpoints for phones and tablets,
//! and multi-touch world camera gestures (1-finger orbit/tap/long-press,
//! 2-finger pinch zoom and pan).

use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Desktop-only mobile preview preset enabled explicitly via `AURORA_MOBILE_PREVIEW`.
/// When unset, desktop behavior remains 100% untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobilePreview {
    PhonePortrait,
    PhoneLandscape,
    Tablet,
}

impl MobilePreview {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "phone-portrait" | "portrait" | "phone" => Some(Self::PhonePortrait),
            "phone-landscape" | "landscape" => Some(Self::PhoneLandscape),
            "tablet" | "foldable" => Some(Self::Tablet),
            _ => None,
        }
    }

    fn simulated_insets(self) -> SafeInsets {
        match self {
            Self::PhonePortrait => SafeInsets {
                top: 32.0,
                bottom: 16.0,
                left: 0.0,
                right: 0.0,
            },
            Self::PhoneLandscape => SafeInsets {
                top: 0.0,
                bottom: 12.0,
                left: 32.0,
                right: 16.0,
            },
            Self::Tablet => SafeInsets {
                top: 24.0,
                bottom: 12.0,
                left: 0.0,
                right: 0.0,
            },
        }
    }

    fn form_factor(self) -> FormFactor {
        match self {
            Self::PhonePortrait => FormFactor::CompactPortrait,
            Self::PhoneLandscape => FormFactor::CompactLandscape,
            Self::Tablet => FormFactor::Medium,
        }
    }
}

/// Active desktop mobile-preview mode from `AURORA_MOBILE_PREVIEW`, if any.
pub fn mobile_preview() -> Option<MobilePreview> {
    static PREVIEW: OnceLock<Option<MobilePreview>> = OnceLock::new();
    *PREVIEW.get_or_init(|| std::env::var("AURORA_MOBILE_PREVIEW").ok().and_then(|v| MobilePreview::parse(&v)))
}

fn preview_ime_dp() -> f32 {
    static IME: OnceLock<f32> = OnceLock::new();
    *IME.get_or_init(|| {
        std::env::var("AURORA_MOBILE_PREVIEW_IME")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(0.0)
    })
}

/// Semantic haptic feedback categories for touch interactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum HapticKind {
    Tap,
    LongPress,
    Confirm,
}

/// Trigger a short haptic pulse on Android when haptics are enabled in mobile
/// preferences. No-op on desktop.
pub fn trigger_haptic(kind: HapticKind) {
    if !crate::ui::mobile_prefs::current().haptics_enabled {
        return;
    }
    #[cfg(target_os = "android")]
    {
        let code = match kind {
            HapticKind::Tap => 0,
            HapticKind::LongPress => 1,
            HapticKind::Confirm => 2,
        };
        crate::android::perform_haptic(code);
    }
    #[cfg(not(target_os = "android"))]
    let _ = kind;
}

/// Minimum recommended touch target size in logical density-independent pixels.
#[allow(dead_code)]
pub const MIN_TOUCH_TARGET_DP: f32 = 48.0;
const TAP_MAX_MOVE_DP: f32 = 18.0;
const TAP_MAX_AGE: Duration = Duration::from_millis(350);
const LONG_PRESS_AGE: Duration = Duration::from_millis(500);

/// Responsive layout size class derived from the logical viewport dimensions
/// and orientation (mobile/tablet only; desktop behavior remains unchanged).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormFactor {
    /// Phone portrait (`width < 600 dp`, `height >= width`): bottom sheets / full-screen sheets,
    /// single active panel at a time.
    CompactPortrait,
    /// Phone landscape (`height < 500 dp`, `width > height`): side drawer preserving 3D view.
    CompactLandscape,
    /// Tablet or foldable (`600 dp <= width < 840 dp`, `height >= 500 dp`).
    Medium,
    /// Large tablet or desktop (`width >= 840 dp`, `height >= 500 dp`).
    Expanded,
}

impl FormFactor {
    /// Classify the current viewport in logical dp.
    pub fn from_viewport(width_dp: f32, height_dp: f32) -> Self {
        if height_dp < 500.0 && width_dp > height_dp {
            Self::CompactLandscape
        } else if width_dp < 600.0 {
            Self::CompactPortrait
        } else if width_dp < 840.0 {
            Self::Medium
        } else {
            Self::Expanded
        }
    }

    /// True on touch-primary platforms (Android) or when the developer
    /// explicitly sets `AURORA_MOBILE_PREVIEW`. Normal desktop runs always
    /// return `false` so desktop layout, styling, and windowing remain 100%
    /// unchanged.
    pub fn is_touch_device() -> bool {
        cfg!(target_os = "android") || mobile_preview().is_some()
    }

    /// True for phone-sized viewports (portrait or landscape).
    pub fn is_phone(self) -> bool {
        matches!(self, Self::CompactPortrait | Self::CompactLandscape)
    }

    /// Maximum width for floating windows/panels so they never clip off small screens.
    #[allow(dead_code)]
    pub fn panel_max_width(self, viewport_width_dp: f32, desktop_width: f32) -> f32 {
        match self {
            Self::CompactPortrait => (viewport_width_dp - 16.0).max(260.0),
            Self::CompactLandscape => (viewport_width_dp * 0.56).clamp(300.0, 520.0).min(viewport_width_dp),
            Self::Medium => desktop_width.min((viewport_width_dp - 32.0).max(320.0)),
            Self::Expanded => desktop_width,
        }
    }
}

/// Safe-area insets (display cutout / notch / visible system bars) in logical dp.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SafeInsets {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

impl SafeInsets {
    pub const ZERO: Self = Self {
        top: 0.0,
        bottom: 0.0,
        left: 0.0,
        right: 0.0,
    };
}

/// Computed responsive viewport geometry for the current frame, accounting for
/// safe-area insets and the on-screen virtual keyboard (IME).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResponsiveLayout {
    pub form_factor: FormFactor,
    pub screen_rect: egui::Rect,
    pub safe_insets: SafeInsets,
    pub ime_bottom_dp: f32,
    pub usable_rect: egui::Rect,
}

impl ResponsiveLayout {
    /// Compute the responsive layout from raw viewport rect, safe insets, and IME height (in dp).
    pub fn compute(screen_rect: egui::Rect, safe_insets: SafeInsets, ime_bottom_dp: f32) -> Self {
        let form_factor = FormFactor::from_viewport(screen_rect.width(), screen_rect.height());
        let left = (screen_rect.left() + safe_insets.left.max(0.0)).min(screen_rect.right());
        let right = (screen_rect.right() - safe_insets.right.max(0.0)).max(left);
        let top = (screen_rect.top() + safe_insets.top.max(0.0)).min(screen_rect.bottom());
        let bottom_inset = safe_insets.bottom.max(ime_bottom_dp).max(0.0);
        let bottom = (screen_rect.bottom() - bottom_inset).max(top);
        let usable_rect = egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom));
        Self {
            form_factor,
            screen_rect,
            safe_insets,
            ime_bottom_dp: ime_bottom_dp.max(0.0),
            usable_rect,
        }
    }

    /// Build the responsive layout for the current egui frame.
    /// On desktop platforms, insets and IME height are zero so `usable_rect == content_rect`.
    pub fn from_ctx(ctx: &egui::Context) -> Self {
        let screen = ctx.content_rect();
        #[cfg(target_os = "android")]
        let (safe_insets, ime_bottom_dp) = {
            let ppp = ctx.pixels_per_point().max(1.0);
            let ([top, bottom, left, right], ime_px) = crate::android::window_insets_px();
            (
                SafeInsets {
                    top: top / ppp,
                    bottom: bottom / ppp,
                    left: left / ppp,
                    right: right / ppp,
                },
                ime_px / ppp,
            )
        };
        #[cfg(not(target_os = "android"))]
        let (safe_insets, ime_bottom_dp) = match mobile_preview() {
            Some(preview) => (preview.simulated_insets(), preview_ime_dp()),
            None => (SafeInsets::ZERO, 0.0),
        };
        let mut layout = Self::compute(screen, safe_insets, ime_bottom_dp);
        if let Some(preview) = mobile_preview() {
            layout.form_factor = preview.form_factor();
        }
        layout
    }

    /// True when the virtual keyboard (IME) is currently visible.
    #[allow(dead_code)]
    pub fn ime_visible(&self) -> bool {
        self.ime_bottom_dp > 1.0
    }
}

/// Apply touch-friendly spacing, hit-target minimums, and discreet floating
/// scrollbars on touch devices. No-op on desktop.
pub fn apply_touch_style(style: &mut egui::Style, form_factor: FormFactor) {
    if !FormFactor::is_touch_device() {
        return;
    }
    apply_touch_style_inner(style, form_factor);
}

fn apply_touch_style_inner(style: &mut egui::Style, form_factor: FormFactor) {
    let min_h = match form_factor {
        FormFactor::CompactPortrait | FormFactor::CompactLandscape => 44.0,
        FormFactor::Medium => 40.0,
        FormFactor::Expanded => 36.0,
    };
    style.spacing.interact_size.y = style.spacing.interact_size.y.max(min_h);
    style.spacing.interact_size.x = style.spacing.interact_size.x.max(min_h);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(8);
    style.spacing.slider_width = style.spacing.slider_width.max(160.0);
    style.spacing.scroll = egui::style::ScrollStyle::floating();
    style.spacing.scroll.bar_width = 6.0;
    style.interaction.selectable_labels = false;
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
    /// Optional finger ID currently owned by the virtual thumbstick so
    /// dual-thumb controls never fight with camera orbit/pinch gestures.
    reserved_finger: Option<u64>,
}

impl TouchNav {
    /// Reserve or release a finger ID owned by another touch control (e.g. virtual thumbstick).
    pub fn set_reserved_finger(&mut self, id: Option<u64>) {
        self.reserved_finger = id;
        if let Some(reserved) = id {
            self.fingers.retain(|f| f.id != reserved);
        }
    }

    /// Process a raw touch event (`Start`, `Move`, `End`, `Cancel`).
    pub fn on_touch(&mut self, phase: winit::event::TouchPhase, id: u64, pos: [f32; 2], now: Instant) -> TouchAction {
        if self.reserved_finger == Some(id) {
            return TouchAction::default();
        }
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
        self.reserved_finger = None;
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
        assert_eq!(FormFactor::from_viewport(390.0, 844.0), FormFactor::CompactPortrait);
        assert_eq!(FormFactor::from_viewport(844.0, 390.0), FormFactor::CompactLandscape);
        assert_eq!(FormFactor::from_viewport(720.0, 400.0), FormFactor::CompactLandscape);
        assert_eq!(FormFactor::from_viewport(768.0, 1024.0), FormFactor::Medium);
        assert_eq!(FormFactor::from_viewport(1024.0, 768.0), FormFactor::Expanded);
        assert!(FormFactor::CompactPortrait.is_phone());
        assert!(FormFactor::CompactLandscape.is_phone());
        assert!(!FormFactor::Medium.is_phone());
        assert!(!FormFactor::Expanded.is_phone());
        assert!(FormFactor::CompactPortrait.panel_max_width(360.0, 500.0) <= 344.0);
    }

    #[test]
    fn responsive_layout_shrinks_usable_rect_by_safe_insets_and_ime() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(390.0, 844.0));
        let insets = SafeInsets {
            top: 44.0,
            bottom: 24.0,
            left: 0.0,
            right: 0.0,
        };
        let without_ime = ResponsiveLayout::compute(screen, insets, 0.0);
        assert_eq!(without_ime.form_factor, FormFactor::CompactPortrait);
        assert!(!without_ime.ime_visible());
        assert_eq!(
            without_ime.usable_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 44.0), egui::pos2(390.0, 820.0))
        );

        let with_ime = ResponsiveLayout::compute(screen, insets, 300.0);
        assert!(with_ime.ime_visible());
        assert_eq!(
            with_ime.usable_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 44.0), egui::pos2(390.0, 544.0))
        );
    }

    #[test]
    fn touch_style_enforces_ergonomic_targets_and_leaves_desktop_unchanged() {
        let mut desktop_style = egui::Style::default();
        let before = desktop_style.spacing.interact_size;
        apply_touch_style(&mut desktop_style, FormFactor::CompactPortrait);
        if !FormFactor::is_touch_device() {
            assert_eq!(desktop_style.spacing.interact_size, before);
        }

        let mut mobile_style = egui::Style::default();
        apply_touch_style_inner(&mut mobile_style, FormFactor::CompactPortrait);
        assert!(mobile_style.spacing.interact_size.y >= 44.0);
        assert!(mobile_style.spacing.scroll.floating);
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

    #[test]
    fn reserved_finger_is_ignored_while_second_finger_orbits_camera() {
        let mut nav = TouchNav::default();
        let t0 = Instant::now();
        nav.set_reserved_finger(Some(10));
        assert_eq!(nav.on_touch(TouchPhase::Started, 10, [40.0, 300.0], t0), TouchAction::default());
        assert_eq!(
            nav.on_touch(TouchPhase::Moved, 10, [80.0, 260.0], t0 + Duration::from_millis(16)),
            TouchAction::default()
        );
        // Second finger still orbits the camera simultaneously.
        nav.on_touch(TouchPhase::Started, 20, [300.0, 200.0], t0);
        let drag = nav.on_touch(TouchPhase::Moved, 20, [340.0, 210.0], t0 + Duration::from_millis(20));
        assert_eq!(drag.orbit, [40.0, 10.0]);
    }
}
