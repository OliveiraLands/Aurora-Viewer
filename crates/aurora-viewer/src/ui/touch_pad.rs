//! Mobile-only virtual thumbstick and right-thumb game action cluster.
//!
//! Strictly isolated for touch devices (`FormFactor::is_touch_device()`):
//! desktop platforms never render or process this module.
//!
//! - Left thumbstick: Floating (spawns where the left thumb touches in the
//!   bottom-left zone) or Fixed, with deadzone, proportional run threshold,
//!   and upward drag Auto-Run lock.
//! - Right-thumb action cluster: translucent circular game buttons (46-54dp)
//!   for Jump / Rise / Stand, Fly / Land, Descend / Crouch, Mouselook, and
//!   Reset Camera.

use crate::agent::MoveInput;
use crate::theme::Palette;
use crate::touch::{FormFactor, ResponsiveLayout};
use crate::ui::bars::BarAction;
use crate::ui::widgets;
use egui::{CornerRadius, Id, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use winit::event::TouchPhase;

const STICK_OUTER_RADIUS: f32 = 58.0;
const STICK_KNOB_RADIUS: f32 = 24.0;
const STICK_DEADZONE: f32 = 0.16;
const STICK_RUN_THRESHOLD: f32 = 0.82;
const AUTO_RUN_PULL_RATIO: f32 = 1.28;
const PRIMARY_BTN_SIZE: f32 = 54.0;
const SECONDARY_BTN_SIZE: f32 = 46.0;

/// Placement mode for the left virtual thumbstick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum TouchPadMode {
    /// Appears wherever the left thumb touches in the bottom-left movement zone.
    #[default]
    Floating,
    /// Stays anchored in the bottom-left corner.
    Fixed,
}

/// Output actions emitted by the right-thumb action cluster this frame.
#[derive(Default)]
pub struct TouchPadActions {
    pub bar_action: BarAction,
}

/// Persistent runtime state for the mobile virtual gamepad overlay.
#[derive(Debug, Default)]
pub struct TouchPadState {
    pub mode: TouchPadMode,
    finger_id: Option<u64>,
    origin: Pos2,
    knob: Pos2,
    /// Normalized stick vector: `x` in `-1..=1` (right positive), `y` in `-1..=1` (forward/up positive).
    axes: [f32; 2],
    auto_run: bool,
    jump_held: bool,
    down_held: bool,
}

impl TouchPadState {
    /// Active finger ID currently owned by the left thumbstick, if any.
    pub fn active_finger(&self) -> Option<u64> {
        self.finger_id
    }

    /// True when the virtual thumbstick or Auto-Run is actively moving the avatar.
    #[allow(dead_code)]
    pub fn is_moving(&self) -> bool {
        self.auto_run || self.axes[0].abs() > STICK_DEADZONE || self.axes[1].abs() > STICK_DEADZONE
    }

    /// Clear active touch state (e.g. when app suspends, enters VR, or opens a modal).
    pub fn clear(&mut self) {
        self.finger_id = None;
        self.axes = [0.0, 0.0];
        self.jump_held = false;
        self.down_held = false;
    }

    /// Process a raw touch event before `TouchNav` sees it.
    ///
    /// Returns `true` when the touch belongs to the left virtual thumbstick so
    /// the camera gesture recognizer ignores that finger (dual-thumb control).
    pub fn handle_touch(&mut self, phase: TouchPhase, id: u64, pos: [f32; 2], usable: Rect) -> bool {
        let prefs = crate::ui::mobile_prefs::current();
        self.mode = prefs.pad_mode();
        let p = Pos2::new(pos[0], pos[1]);
        match phase {
            TouchPhase::Started => {
                if self.finger_id.is_none() && thumbstick_zone(usable, prefs.left_handed).contains(p) {
                    self.finger_id = Some(id);
                    self.origin = match self.mode {
                        TouchPadMode::Floating => clamp_origin(p, usable, prefs.left_handed),
                        TouchPadMode::Fixed => fixed_origin(usable, prefs.left_handed),
                    };
                    self.update_knob(p);
                    return true;
                }
                false
            }
            TouchPhase::Moved => {
                if self.finger_id == Some(id) {
                    self.update_knob(p);
                    return true;
                }
                false
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if self.finger_id == Some(id) {
                    self.finger_id = None;
                    self.axes = [0.0, 0.0];
                    return true;
                }
                false
            }
        }
    }

    fn update_knob(&mut self, p: Pos2) {
        let delta = p - self.origin;
        let dist = delta.length();
        // Dragging strongly upward locks Auto-Run; pulling back cancels it.
        if -delta.y > STICK_OUTER_RADIUS * AUTO_RUN_PULL_RATIO && delta.x.abs() < STICK_OUTER_RADIUS * 0.65 {
            if !self.auto_run {
                crate::touch::trigger_haptic(crate::touch::HapticKind::Confirm);
            }
            self.auto_run = true;
        } else if delta.y > STICK_OUTER_RADIUS * 0.35 {
            self.auto_run = false;
        }

        let clamped = if dist > STICK_OUTER_RADIUS {
            delta * (STICK_OUTER_RADIUS / dist)
        } else {
            delta
        };
        self.knob = self.origin + clamped;

        let nx = (clamped.x / STICK_OUTER_RADIUS).clamp(-1.0, 1.0);
        // Screen Y grows downward; invert so pushing thumb up means forward (+1).
        let ny = (-clamped.y / STICK_OUTER_RADIUS).clamp(-1.0, 1.0);
        let mag = (nx * nx + ny * ny).sqrt();
        if mag < STICK_DEADZONE {
            self.axes = [0.0, 0.0];
        } else {
            self.axes = [nx, ny];
        }
    }

    /// Merge virtual thumbstick and held action buttons into the frame's `MoveInput`.
    pub fn apply_to_move_input(&self, input: &mut MoveInput) {
        let [x, y] = self.axes;
        let mag = (x * x + y * y).sqrt();
        if self.auto_run || y > STICK_DEADZONE {
            input.forward = true;
        } else if y < -STICK_DEADZONE {
            input.back = true;
        }
        if x < -STICK_DEADZONE {
            input.turn_left = true;
        } else if x > STICK_DEADZONE {
            input.turn_right = true;
        }
        if self.auto_run || mag >= STICK_RUN_THRESHOLD {
            input.run = true;
        }
        if self.jump_held {
            input.up = true;
        }
        if self.down_held {
            input.down = true;
        }
    }

    /// Draw the mobile virtual thumbstick and right-thumb action cluster.
    /// No-op on desktop platforms.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        flying: bool,
        sitting: bool,
        mouselook: bool,
        hud_collapsed: bool,
    ) -> TouchPadActions {
        let mut actions = TouchPadActions::default();
        if !FormFactor::is_touch_device() {
            return actions;
        }
        let layout = ResponsiveLayout::from_ctx(ctx);
        if layout.ime_visible() || widgets::mobile_container::has_active_sheet(ctx) {
            self.jump_held = false;
            self.down_held = false;
            return actions;
        }

        let prefs = crate::ui::mobile_prefs::current();
        let opacity_scale = prefs.pad_opacity.clamp(0.3, 1.0);
        let usable = layout.usable_rect;
        // Reserve room above the compact bottom dock when HUD is expanded.
        let bottom_pad = if hud_collapsed { 18.0 } else { 76.0 };

        // 1. Left thumbstick visual (or right in left-handed mode) + Auto-Run pill.
        let stick_center = if self.finger_id.is_some() {
            self.origin
        } else if prefs.left_handed {
            Pos2::new(
                usable.right() - STICK_OUTER_RADIUS - 22.0,
                usable.bottom() - bottom_pad - STICK_OUTER_RADIUS,
            )
        } else {
            Pos2::new(
                usable.left() + STICK_OUTER_RADIUS + 22.0,
                usable.bottom() - bottom_pad - STICK_OUTER_RADIUS,
            )
        };
        let knob_pos = if self.finger_id.is_some() { self.knob } else { stick_center };
        let active = self.finger_id.is_some() || self.auto_run;
        let ring_alpha = (if active { 0.48 } else { 0.22 }) * opacity_scale;

        egui::Area::new(Id::unique("mobile_touch_stick"))
            .order(egui::Order::Foreground)
            .fixed_pos(usable.min)
            .interactable(false)
            .show(ctx, |ui| {
                let painter = ui.painter();
                painter.circle_filled(stick_center, STICK_OUTER_RADIUS, p.panel.gamma_multiply(ring_alpha));
                painter.circle_stroke(
                    stick_center,
                    STICK_OUTER_RADIUS,
                    Stroke::new(1.5, p.ink.gamma_multiply(if active { 0.45 } else { 0.22 })),
                );
                painter.circle_filled(
                    knob_pos,
                    STICK_KNOB_RADIUS,
                    if self.auto_run {
                        p.violet.gamma_multiply(0.85)
                    } else {
                        p.ink.gamma_multiply(if active { 0.65 } else { 0.35 })
                    },
                );
            });

        // Tappable Auto-Run indicator pill above the thumbstick when locked.
        if self.auto_run {
            let pill_pos = Pos2::new(stick_center.x - 42.0, stick_center.y - STICK_OUTER_RADIUS - 32.0);
            egui::Area::new(Id::unique("mobile_autorun_pill"))
                .order(egui::Order::Foreground)
                .fixed_pos(pill_pos)
                .show(ctx, |ui| {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(84.0, 26.0), Sense::click());
                    ui.painter()
                        .rect_filled(rect, CornerRadius::same(13), p.violet.gamma_multiply(0.85));
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "AUTO-RUN",
                        egui::FontId::proportional(11.0),
                        p.on_accent,
                    );
                    if resp.clicked() {
                        self.auto_run = false;
                    }
                });
        }

        // 2. Right-thumb circular action cluster.
        // Keep the lowest button (down_pos at +32 + SECONDARY_BTN_SIZE * 0.5) clear of the bottom dock.
        let anchor_x = usable.right() - 20.0 - PRIMARY_BTN_SIZE * 0.5;
        let anchor_y = usable.bottom() - bottom_pad - 32.0 - SECONDARY_BTN_SIZE * 0.5;

        let mut jump_held = false;
        let mut down_held = false;

        egui::Area::new(Id::unique("mobile_action_cluster"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(anchor_x - 120.0, anchor_y - 120.0))
            .show(ctx, |ui| {
                let origin = Pos2::new(anchor_x, anchor_y);

                // Primary button: Stand (if sitting), Rise (if flying), or Jump (on ground).
                let primary_icon = if sitting {
                    "walk"
                } else if flying {
                    "arrow-up"
                } else {
                    "arrow-big-up-filled"
                };
                let primary_resp = circle_action_button(ui, p, origin, PRIMARY_BTN_SIZE, primary_icon, false, !sitting);
                if sitting {
                    if primary_resp.clicked() {
                        actions.bar_action = BarAction::StandUp;
                    }
                } else if primary_resp.is_pointer_button_down_on() {
                    jump_held = true;
                }

                // Fly / Land toggle (upper-left of primary button).
                let fly_pos = origin + Vec2::new(-64.0, -20.0);
                if circle_action_button(ui, p, fly_pos, SECONDARY_BTN_SIZE, "wind", flying, false).clicked() {
                    actions.bar_action = BarAction::ToggleFly;
                }

                // Descend / Crouch button (directly left of primary button).
                let down_pos = origin + Vec2::new(-70.0, 32.0);
                let down_icon = if flying { "arrow-down" } else { "chevrons-down" };
                let down_resp = circle_action_button(ui, p, down_pos, SECONDARY_BTN_SIZE, down_icon, false, true);
                if down_resp.is_pointer_button_down_on() {
                    down_held = true;
                }

                // Mouselook / 1st person toggle (above primary button).
                let ml_pos = origin + Vec2::new(-6.0, -68.0);
                if circle_action_button(ui, p, ml_pos, SECONDARY_BTN_SIZE, "eye-filled", mouselook, false).clicked() {
                    actions.bar_action = BarAction::ToggleMouselook;
                }

                // Reset camera button (upper-left arc).
                let reset_pos = origin + Vec2::new(-54.0, -76.0);
                if circle_action_button(ui, p, reset_pos, 40.0, "focus-centered", false, false).clicked() {
                    actions.bar_action = BarAction::ResetCamera;
                }
            });

        self.jump_held = jump_held;
        self.down_held = down_held;
        actions
    }
}

fn thumbstick_zone(usable: Rect, left_handed: bool) -> Rect {
    if left_handed {
        Rect::from_min_max(
            Pos2::new(usable.right() - usable.width() * 0.42, usable.top() + usable.height() * 0.42),
            usable.max,
        )
    } else {
        Rect::from_min_max(
            Pos2::new(usable.left(), usable.top() + usable.height() * 0.42),
            Pos2::new(usable.left() + usable.width() * 0.42, usable.bottom()),
        )
    }
}

fn fixed_origin(usable: Rect, left_handed: bool) -> Pos2 {
    let x = if left_handed {
        usable.right() - STICK_OUTER_RADIUS - 22.0
    } else {
        usable.left() + STICK_OUTER_RADIUS + 22.0
    };
    Pos2::new(x, usable.bottom() - 76.0 - STICK_OUTER_RADIUS)
}

fn clamp_origin(pos: Pos2, usable: Rect, left_handed: bool) -> Pos2 {
    let margin = STICK_OUTER_RADIUS + 8.0;
    let (min_x, max_x) = if left_handed {
        let max_x = (usable.right() - margin).max(usable.left());
        let min_x = (usable.right() - usable.width() * 0.42).min(max_x);
        (min_x, max_x)
    } else {
        let min_x = usable.left() + margin;
        let max_x = (usable.left() + usable.width() * 0.42).max(min_x);
        (min_x, max_x)
    };
    let min_y = usable.top() + usable.height() * 0.42;
    let max_y = (usable.bottom() - margin).max(min_y);
    Pos2::new(pos.x.clamp(min_x, max_x), pos.y.clamp(min_y, max_y))
}

fn circle_action_button(
    ui: &mut egui::Ui,
    p: &Palette,
    center: Pos2,
    diameter: f32,
    icon: &str,
    active: bool,
    holdable: bool,
) -> egui::Response {
    let rect = Rect::from_center_size(center, Vec2::splat(diameter));
    let id = ui.make_persistent_id(("touch_pad_btn", icon, diameter as i32));
    let sense = if holdable { Sense::click_and_drag() } else { Sense::click() };
    let resp = ui.interact(rect, id, sense);
    let pressed = resp.is_pointer_button_down_on();
    let fill = if active {
        p.violet.gamma_multiply(0.75)
    } else if pressed {
        p.raised.gamma_multiply(0.85)
    } else {
        p.panel.gamma_multiply(0.58)
    };
    let stroke_color = if active || pressed {
        p.violet_light.gamma_multiply(0.85)
    } else {
        p.ink.gamma_multiply(0.28)
    };
    let radius = diameter * 0.5;
    ui.painter().circle_filled(center, radius, fill);
    ui.painter().circle_stroke(center, radius, Stroke::new(1.5, stroke_color));
    let tint = if active { p.on_accent } else { p.ink };
    if crate::ui::icons::global(icon).is_some() {
        widgets::paint_glyph(ui.painter(), icon, center, diameter * 0.44, tint);
    } else {
        ui.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            RichText::new("•").text(),
            egui::FontId::proportional(16.0),
            tint,
        );
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbstick_claims_bottom_left_touch_and_drives_move_input() {
        let usable = Rect::from_min_size(Pos2::ZERO, Vec2::new(844.0, 390.0));
        let mut pad = TouchPadState::default();

        // Touch in right half is NOT claimed (belongs to camera orbit).
        assert!(!pad.handle_touch(TouchPhase::Started, 1, [600.0, 260.0], usable));
        assert_eq!(pad.active_finger(), None);

        // Touch in bottom-left zone IS claimed.
        assert!(pad.handle_touch(TouchPhase::Started, 2, [100.0, 280.0], usable));
        assert_eq!(pad.active_finger(), Some(2));

        // Dragging strongly upward activates forward + run + auto-run.
        assert!(pad.handle_touch(TouchPhase::Moved, 2, [100.0, 190.0], usable));
        let mut input = MoveInput::default();
        pad.apply_to_move_input(&mut input);
        assert!(input.forward);
        assert!(input.run);
        assert!(pad.auto_run);

        // Releasing finger keeps auto-run active until pulled back.
        assert!(pad.handle_touch(TouchPhase::Ended, 2, [100.0, 190.0], usable));
        assert_eq!(pad.active_finger(), None);
        let mut after_release = MoveInput::default();
        pad.apply_to_move_input(&mut after_release);
        assert!(after_release.forward);
        assert!(after_release.run);
    }

    #[test]
    fn thumbstick_respects_deadzone_and_turns_left_or_right() {
        let usable = Rect::from_min_size(Pos2::ZERO, Vec2::new(390.0, 844.0));
        let mut pad = TouchPadState::default();
        assert!(pad.handle_touch(TouchPhase::Started, 5, [90.0, 700.0], usable));
        // Tiny jitter inside deadzone produces no movement.
        assert!(pad.handle_touch(TouchPhase::Moved, 5, [94.0, 698.0], usable));
        let mut idle = MoveInput::default();
        pad.apply_to_move_input(&mut idle);
        assert!(!idle.forward && !idle.back && !idle.turn_left && !idle.turn_right);

        // Pulling right activates turn_right.
        assert!(pad.handle_touch(TouchPhase::Moved, 5, [130.0, 700.0], usable));
        let mut right = MoveInput::default();
        pad.apply_to_move_input(&mut right);
        assert!(right.turn_right);
        assert!(!right.turn_left);
    }
}
