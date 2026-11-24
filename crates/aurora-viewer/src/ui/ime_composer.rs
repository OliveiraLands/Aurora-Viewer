//! Mobile-only virtual keyboard (IME) floating composer and global TextEdit
//! auto-scroll plugin.
//!
//! Strictly isolated for touch devices (`FormFactor::is_touch_device()`):
//! desktop platforms never render the floating composer or alter scroll
//! behavior.
//!
//! - `install(ctx)`: installs `MobileImePlugin`, tracking the focused text
//!   widget rect each frame so `keep_focused_visible(ui)` scrolls any focused
//!   `TextEdit` cleanly above the virtual keyboard (`usable_rect`).
//! - `show_chat_composer(...)`: when the virtual keyboard is open in-world,
//!   anchors a touch-friendly chat composer immediately above the keyboard
//!   (with Whisper/Say/Shout cycle, 48dp Send button, and Hide Keyboard button;
//!   automatically switches to a compact single-strip layout in
//!   `CompactLandscape`).

use crate::theme::Palette;
use crate::touch::{FormFactor, ResponsiveLayout};
use crate::ui::chat::OutgoingChat;
use crate::ui::widgets;
use aurora_net::ChatType;
use egui::{Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, TextEdit, Vec2};

const PORTRAIT_COMPOSER_H: f32 = 54.0;
const LANDSCAPE_COMPOSER_H: f32 = 44.0;

#[derive(Clone, Copy, Default)]
struct FocusedTextState {
    id: Option<Id>,
    rect: Option<Rect>,
}

fn focused_state_id() -> Id {
    Id::unique("mobile_ime_focused_text")
}

fn chat_type_id() -> Id {
    Id::unique("mobile_ime_chat_type")
}

/// Install the global mobile IME auto-scroll observer plugin on `ctx`.
/// No-op at runtime on desktop platforms.
pub fn install(ctx: &egui::Context) {
    ctx.add_plugin(MobileImePlugin);
}

struct MobileImePlugin;

impl egui::Plugin for MobileImePlugin {
    fn debug_name(&self) -> &'static str {
        "mobile_ime_autoscroll"
    }

    fn on_end_pass(&mut self, ui: &mut egui::Ui) {
        if !FormFactor::is_touch_device() {
            return;
        }
        let ctx = ui.ctx();
        let focused = if ctx.egui_wants_keyboard_input() {
            ctx.memory(|m| m.focused())
        } else {
            None
        };
        let rect = focused.and_then(|id| {
            ctx.viewport(|vp| {
                vp.this_pass
                    .widgets
                    .get(id)
                    .filter(|w| w.visible && w.rect.is_positive())
                    .map(|w| w.rect)
            })
        });
        ctx.data_mut(|d| {
            d.insert_temp(focused_state_id(), FocusedTextState { id: focused, rect });
        });
    }
}

/// Pure helper: returns the vertical adjustment needed if `widget_rect` is
/// obscured below `usable_rect` (e.g. behind the virtual keyboard) or above it.
pub(crate) fn obscured_below_usable(widget_rect: Rect, usable_rect: Rect) -> bool {
    widget_rect.bottom() > usable_rect.bottom() - 8.0 || widget_rect.top() < usable_rect.top()
}

/// Call inside a mobile scrollable container (`ScrollArea`) to automatically
/// keep any focused `TextEdit` visible above the virtual keyboard.
/// No-op on desktop platforms.
pub fn keep_focused_visible(ui: &mut egui::Ui) {
    if !FormFactor::is_touch_device() {
        return;
    }
    let layout = ResponsiveLayout::from_ctx(ui.ctx());
    if !layout.ime_visible() {
        return;
    }
    let state = ui
        .ctx()
        .data(|d| d.get_temp::<FocusedTextState>(focused_state_id()))
        .unwrap_or_default();
    if let (Some(_id), Some(rect)) = (state.id, state.rect)
        && obscured_below_usable(rect, layout.usable_rect)
    {
        ui.scroll_to_rect(rect.expand(12.0), Some(egui::Align::Center));
    }
}

/// Pure geometry helper for the floating IME chat composer anchored directly
/// above `layout.usable_rect.bottom()`.
pub(crate) fn composer_rect(layout: ResponsiveLayout) -> Rect {
    let h = if layout.form_factor == FormFactor::CompactLandscape {
        LANDSCAPE_COMPOSER_H
    } else {
        PORTRAIT_COMPOSER_H
    };
    let usable = layout.usable_rect;
    Rect::from_min_max(
        Pos2::new(usable.left(), (usable.bottom() - h).max(usable.top())),
        Pos2::new(usable.right(), usable.bottom()),
    )
}

fn next_chat_type(current: ChatType) -> ChatType {
    match current {
        ChatType::Normal => ChatType::Shout,
        ChatType::Shout => ChatType::Whisper,
        _ => ChatType::Normal,
    }
}

fn chat_type_icon(chat_type: ChatType) -> &'static str {
    match chat_type {
        ChatType::Whisper => "volume-2",
        ChatType::Shout => "speakerphone",
        _ => "message-circle",
    }
}

/// Floating chat composer anchored immediately above the Android virtual
/// keyboard (`IME`). Returns `Some(OutgoingChat)` when the user sends a line.
/// No-op on desktop platforms.
pub fn show_chat_composer(ctx: &egui::Context, p: &Palette, draft: &mut String) -> Option<OutgoingChat> {
    if !FormFactor::is_touch_device() {
        return None;
    }
    let layout = ResponsiveLayout::from_ctx(ctx);
    if !layout.ime_visible() {
        return None;
    }

    let rect = composer_rect(layout);
    let compact = layout.form_factor == FormFactor::CompactLandscape;
    let btn_h = if compact { 34.0 } else { 42.0 };
    let mut chat_type = ctx.data(|d| d.get_temp::<ChatType>(chat_type_id())).unwrap_or(ChatType::Normal);
    let mut sent = None;

    let bg = Color32::from_rgba_unmultiplied(p.bar.r(), p.bar.g(), p.bar.b(), 245);
    let frame = egui::Frame::new()
        .fill(bg)
        .stroke(Stroke::new(1.0, p.muted_dim.gamma_multiply(0.5)))
        .corner_radius(CornerRadius {
            nw: 12,
            ne: 12,
            sw: 0,
            se: 0,
        })
        .inner_margin(egui::Margin::symmetric(8, if compact { 4 } else { 6 }));

    egui::Area::new(Id::unique("mobile_ime_composer"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .movable(false)
        .show(ctx, |ui| {
            frame.show(ui, |ui| {
                let inner_w = (rect.width() - 16.0).max(160.0);
                ui.set_min_width(inner_w);
                ui.set_max_width(inner_w);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;

                    // Whisper / Say / Shout mode cycle button.
                    if composer_icon_btn(ui, p, chat_type_icon(chat_type), Vec2::new(btn_h, btn_h), false).clicked() {
                        chat_type = next_chat_type(chat_type);
                        ctx.data_mut(|d| d.insert_temp(chat_type_id(), chat_type));
                    }

                    // Input field taking remaining width.
                    let send_w = if compact { 44.0 } else { 48.0 };
                    let hide_w = btn_h;
                    let field_w = (inner_w - btn_h - send_w - hide_w - 18.0).max(80.0);
                    let edit = ui.add_sized(
                        Vec2::new(field_w, btn_h),
                        TextEdit::singleline(draft).hint_text(aurora_i18n::tr!("ui-chat-say")),
                    );
                    let enter_pressed = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    // Send button (48dp primary accent).
                    let send_clicked = composer_icon_btn(ui, p, "send", Vec2::new(send_w, btn_h), true).clicked();
                    if (enter_pressed || send_clicked) && !draft.trim().is_empty() {
                        let message = std::mem::take(draft);
                        sent = Some(OutgoingChat {
                            message,
                            channel: 0,
                            chat_type,
                        });
                    }

                    // Hide soft keyboard button.
                    if composer_icon_btn(ui, p, "keyboard-hide", Vec2::new(hide_w, btn_h), false).clicked() {
                        if let Some(id) = ctx.memory(|m| m.focused()) {
                            ctx.memory_mut(|m| m.surrender_focus(id));
                        }
                        #[cfg(target_os = "android")]
                        crate::android::hide_soft_keyboard();
                    }
                });
            });
        });

    sent
}

fn composer_icon_btn(ui: &mut egui::Ui, p: &Palette, icon: &str, size: Vec2, primary: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if primary {
        if resp.is_pointer_button_down_on() {
            p.violet_light
        } else {
            p.violet
        }
    } else if resp.is_pointer_button_down_on() {
        p.raised
    } else {
        p.field
    };
    ui.painter().rect_filled(rect, CornerRadius::same(10), fill);
    widgets::paint_glyph(ui.painter(), icon, rect.center(), 18.0, if primary { p.on_accent } else { p.ink });
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::touch::SafeInsets;

    #[test]
    fn composer_rect_anchors_above_ime_and_compacts_in_landscape() {
        let portrait = ResponsiveLayout::compute(Rect::from_min_size(Pos2::ZERO, Vec2::new(390.0, 844.0)), SafeInsets::ZERO, 300.0);
        let p_rect = composer_rect(portrait);
        assert_eq!(p_rect.bottom(), portrait.usable_rect.bottom());
        assert_eq!(p_rect.height(), PORTRAIT_COMPOSER_H);
        assert_eq!(p_rect.width(), 390.0);

        let landscape = ResponsiveLayout::compute(Rect::from_min_size(Pos2::ZERO, Vec2::new(844.0, 390.0)), SafeInsets::ZERO, 210.0);
        let l_rect = composer_rect(landscape);
        assert_eq!(l_rect.bottom(), landscape.usable_rect.bottom());
        assert_eq!(l_rect.height(), LANDSCAPE_COMPOSER_H);
    }

    #[test]
    fn obscured_below_usable_detects_fields_covered_by_keyboard() {
        let usable = Rect::from_min_max(Pos2::new(0.0, 44.0), Pos2::new(390.0, 520.0));
        let visible_field = Rect::from_min_size(Pos2::new(20.0, 200.0), Vec2::new(300.0, 36.0));
        let hidden_field = Rect::from_min_size(Pos2::new(20.0, 540.0), Vec2::new(300.0, 36.0));
        assert!(!obscured_below_usable(visible_field, usable));
        assert!(obscured_below_usable(hidden_field, usable));
    }

    #[test]
    fn chat_composer_is_noop_on_desktop() {
        let ctx = egui::Context::default();
        let p = crate::theme::Theme::default().palette();
        let mut draft = String::from("hello");
        let mut sent = None;
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            sent = show_chat_composer(ui.ctx(), &p, &mut draft);
        });
        if !FormFactor::is_touch_device() {
            assert!(sent.is_none());
            assert!(output.shapes.is_empty());
        }
    }
}
