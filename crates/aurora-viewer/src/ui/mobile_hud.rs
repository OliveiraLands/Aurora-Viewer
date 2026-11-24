//! Mobile-only collapsible game-style HUD.
//!
//! Strictly isolated for touch devices (`FormFactor::is_touch_device()`):
//! desktop bars (`ui/bars/*`) remain 100% unchanged for clean upstream merges.
//!
//! Includes:
//! - Compact top bar: menu/logo button, region pill, mini FPS indicator, and
//!   Collapse/Expand HUD ("Cinema Mode") toggle.
//! - Mini-chat overlay: last 3 local chat lines in translucent pills (tap to
//!   open full Chat sheet).
//! - Compact horizontally scrollable bottom dock with 44dp touch targets.

use crate::theme::Palette;
use crate::touch::{FormFactor, ResponsiveLayout};
use crate::ui::widgets;
use egui::{Color32, CornerRadius, Id, Pos2, Rect, RichText, Sense, Stroke, Vec2};

const TOP_BAR_H: f32 = 38.0;
const DOCK_BTN_SIZE: f32 = 44.0;
const MINI_CHAT_MAX_LINES: usize = 3;

/// Persistent state of the collapsible mobile HUD.
#[derive(Debug, Default)]
pub struct MobileHudState {
    /// When `true`, hides top/bottom HUD chrome and mini-chat ("Cinema Mode"),
    /// leaving only a discreet corner restore button.
    pub collapsed: bool,
}

/// Read-only snapshot passed into `MobileHudState::show` each frame.
pub struct MobileHudContext<'a> {
    pub region_name: &'a str,
    pub fps: f32,
    pub unread_chat: usize,
    pub voice_connected: bool,
    pub mic_on: bool,
    pub recent_chat: Vec<(&'a str, &'a str)>,
}

/// Actions triggered by tapping mobile HUD controls.
#[derive(Debug, Default)]
pub struct MobileHudAction {
    pub open_chat: bool,
    pub open_inventory: bool,
    pub open_people: bool,
    pub open_places: bool,
    pub open_worldmap: bool,
    pub open_appearance: bool,
    pub open_gestures: bool,
    pub toggle_minimap: bool,
    pub open_search: bool,
    pub open_photo: bool,
    pub open_options: bool,
    pub toggle_mic: bool,
}

impl MobileHudAction {
    /// Apply any panel-open requests triggered by the mobile HUD.
    pub fn apply(self, ctx: &egui::Context, panels: &mut super::Panels) {
        if self.open_chat {
            widgets::open_floater(ctx, "conversations", &mut panels.chat);
        }
        if self.open_inventory {
            widgets::open_floater(ctx, "inventory", &mut panels.inventory);
        }
        if self.open_people {
            widgets::open_floater(ctx, "people", &mut panels.people);
        }
        if self.open_places {
            widgets::open_floater(ctx, "places", &mut panels.places);
        }
        if self.open_worldmap {
            widgets::open_floater(ctx, "worldmap", &mut panels.world_map);
        }
        if self.open_appearance {
            widgets::open_floater(ctx, "appearance", &mut panels.appearance);
        }
        if self.open_gestures {
            widgets::open_floater(ctx, "gestures", &mut panels.gestures);
        }
        if self.toggle_minimap {
            panels.minimap = !panels.minimap;
        }
        if self.open_search {
            widgets::open_floater(ctx, crate::ui::search::WINDOW, &mut panels.search);
        }
        if self.open_photo {
            widgets::open_floater(ctx, crate::ui::snapshot::WINDOW, &mut panels.snapshot);
        }
        if self.open_options {
            widgets::open_floater(ctx, crate::ui::options::WINDOW, &mut panels.settings);
        }
    }
}

impl MobileHudState {
    /// Draw the collapsible game-style mobile HUD. No-op on desktop platforms.
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, info: &MobileHudContext<'_>) -> MobileHudAction {
        let mut action = MobileHudAction::default();
        if !FormFactor::is_touch_device() {
            return action;
        }
        let layout = ResponsiveLayout::from_ctx(ctx);
        if layout.ime_visible() {
            return action;
        }
        let usable = layout.usable_rect;

        // Cinema Mode (HUD collapsed): show only a single discreet restore button.
        if self.collapsed {
            let btn_pos = Pos2::new(usable.right() - 48.0, usable.top() + 8.0);
            egui::Area::new(Id::unique("mobile_hud_restore"))
                .order(egui::Order::Foreground)
                .fixed_pos(btn_pos)
                .show(ctx, |ui| {
                    if hud_icon_pill(ui, p, "eye", 40.0, false).clicked() {
                        self.collapsed = false;
                    }
                });
            return action;
        }

        // 1. Compact Top Bar (Left: menu + region pill; Right: FPS + Cinema toggle).
        egui::Area::new(Id::unique("mobile_hud_top"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(usable.left() + 8.0, usable.top() + 6.0))
            .show(ctx, |ui| {
                let row_w = (usable.width() - 16.0).max(200.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(row_w, TOP_BAR_H),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if hud_icon_pill(ui, p, "layout-grid-filled", TOP_BAR_H, false).clicked() {
                            action.open_options = true;
                        }
                        let region_label = if info.region_name.is_empty() {
                            crate::brand::APP_NAME
                        } else {
                            info.region_name
                        };
                        let pill_w = (row_w * 0.45).clamp(110.0, 220.0);
                        if region_pill(ui, p, region_label, pill_w, TOP_BAR_H).clicked() {
                            action.open_worldmap = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            if hud_icon_pill(ui, p, "eye-off", TOP_BAR_H, false).clicked() {
                                self.collapsed = true;
                            }
                            if info.voice_connected {
                                let mic_icon = if info.mic_on { "microphone" } else { "microphone-off" };
                                if hud_icon_pill(ui, p, mic_icon, TOP_BAR_H, info.mic_on).clicked() {
                                    action.toggle_mic = true;
                                }
                            }
                            fps_badge(ui, p, info.fps);
                        });
                    },
                );
            });

        // Hide mini-chat and bottom dock while a phone sheet/drawer is open so
        // the active sheet owns the screen cleanly.
        if widgets::mobile_container::has_active_sheet(ctx) {
            return action;
        }

        // 2. Mini-Chat Overlay (top-left, below top bar, max 3 recent lines).
        if !info.recent_chat.is_empty() {
            let chat_w = (usable.width() * 0.58).clamp(180.0, 320.0);
            let chat_pos = Pos2::new(usable.left() + 10.0, usable.top() + TOP_BAR_H + 14.0);
            egui::Area::new(Id::unique("mobile_mini_chat"))
                .order(egui::Order::Foreground)
                .fixed_pos(chat_pos)
                .show(ctx, |ui| {
                    let frame = egui::Frame::new()
                        .fill(Color32::from_rgba_unmultiplied(p.panel.r(), p.panel.g(), p.panel.b(), 155))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(8, 6));
                    let resp = frame
                        .show(ui, |ui| {
                            ui.set_max_width(chat_w);
                            ui.spacing_mut().item_spacing.y = 2.0;
                            for (sender, msg) in info.recent_chat.iter().take(MINI_CHAT_MAX_LINES) {
                                let line = if sender.is_empty() {
                                    (*msg).to_owned()
                                } else {
                                    format!("{sender}: {msg}")
                                };
                                ui.add(egui::Label::new(RichText::new(line).size(11.5).color(p.ink)).truncate());
                            }
                        })
                        .response
                        .interact(Sense::click());
                    if resp.clicked() {
                        action.open_chat = true;
                    }
                });
        }

        // 3. Compact Scrollable Bottom Dock (centered pill bar; wider in landscape to show more icons).
        let max_dock_w = if layout.form_factor == FormFactor::CompactPortrait {
            460.0
        } else {
            (usable.width() - 280.0).max(520.0)
        };
        let dock_w = (usable.width() - 20.0).clamp(200.0, max_dock_w);
        let dock_h = DOCK_BTN_SIZE + 10.0;
        let dock_pos = Pos2::new(usable.center().x - dock_w * 0.5, usable.bottom() - dock_h - 6.0);
        egui::Area::new(Id::unique("mobile_hud_dock"))
            .order(egui::Order::Foreground)
            .fixed_pos(dock_pos)
            .show(ctx, |ui| {
                let bg = Color32::from_rgba_unmultiplied(p.bar.r(), p.bar.g(), p.bar.b(), 215);
                egui::Frame::new()
                    .fill(bg)
                    .stroke(Stroke::new(1.0, p.ink.gamma_multiply(0.12)))
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(egui::Margin::symmetric(8, 5))
                    .show(ui, |ui| {
                        ui.set_max_width(dock_w - 16.0);
                        widgets::horizontal_scrollable_row(ui, "mobile_bottom_dock", bg, |ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            if dock_button(ui, p, "messages-filled", info.unread_chat).clicked() {
                                action.open_chat = true;
                            }
                            if dock_button(ui, p, "folder-filled", 0).clicked() {
                                action.open_inventory = true;
                            }
                            if dock_button(ui, p, "users-group", 0).clicked() {
                                action.open_people = true;
                            }
                            if dock_button(ui, p, "map-pin-filled", 0).clicked() {
                                action.open_places = true;
                            }
                            if dock_button(ui, p, "map", 0).clicked() {
                                action.open_worldmap = true;
                            }
                            if dock_button(ui, p, "compass-filled", 0).clicked() {
                                action.toggle_minimap = true;
                            }
                            if dock_button(ui, p, "shirt-filled", 0).clicked() {
                                action.open_appearance = true;
                            }
                            if dock_button(ui, p, "hand-click", 0).clicked() {
                                action.open_gestures = true;
                            }
                            if dock_button(ui, p, "search-filled", 0).clicked() {
                                action.open_search = true;
                            }
                            if dock_button(ui, p, "camera-filled", 0).clicked() {
                                action.open_photo = true;
                            }
                            if dock_button(ui, p, "settings-filled", 0).clicked() {
                                action.open_options = true;
                            }
                        });
                    });
            });

        action
    }
}

fn hud_icon_pill(ui: &mut egui::Ui, p: &Palette, icon: &str, size: f32, active: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let fill = if active {
        p.violet.gamma_multiply(0.75)
    } else if resp.is_pointer_button_down_on() {
        p.raised.gamma_multiply(0.85)
    } else {
        Color32::from_rgba_unmultiplied(p.bar.r(), p.bar.g(), p.bar.b(), 195)
    };
    ui.painter().rect_filled(rect, CornerRadius::same((size * 0.3) as u8), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same((size * 0.3) as u8),
        Stroke::new(1.0, p.ink.gamma_multiply(0.12)),
        egui::StrokeKind::Inside,
    );
    widgets::paint_glyph(ui.painter(), icon, rect.center(), 18.0, if active { p.on_accent } else { p.ink });
    resp
}

fn region_pill(ui: &mut egui::Ui, p: &Palette, name: &str, max_w: f32, height: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(max_w, height), Sense::click());
    let fill = if resp.is_pointer_button_down_on() {
        p.raised.gamma_multiply(0.85)
    } else {
        Color32::from_rgba_unmultiplied(p.bar.r(), p.bar.g(), p.bar.b(), 195)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(12), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(12),
        Stroke::new(1.0, p.ink.gamma_multiply(0.12)),
        egui::StrokeKind::Inside,
    );
    widgets::paint_glyph(
        ui.painter(),
        "map-pin-filled",
        Pos2::new(rect.left() + 16.0, rect.center().y),
        14.0,
        p.violet_light,
    );
    widgets::paint_elided(
        ui,
        name,
        Pos2::new(rect.left() + 28.0, rect.center().y),
        (rect.width() - 36.0).max(24.0),
        egui::FontId::proportional(12.5),
        p.ink,
    );
    resp
}

fn fps_badge(ui: &mut egui::Ui, p: &Palette, fps: f32) {
    let text = format!("{:.0} FPS", fps.max(0.0));
    let (rect, _) = ui.allocate_exact_size(Vec2::new(56.0, TOP_BAR_H), Sense::hover());
    let fill = Color32::from_rgba_unmultiplied(p.bar.r(), p.bar.g(), p.bar.b(), 180);
    ui.painter().rect_filled(rect, CornerRadius::same(10), fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(11.0),
        p.muted,
    );
}

fn dock_button(ui: &mut egui::Ui, p: &Palette, icon: &str, badge: usize) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(DOCK_BTN_SIZE), Sense::click());
    let fill = if resp.is_pointer_button_down_on() {
        p.raised
    } else if resp.hovered() {
        p.raised.gamma_multiply(0.6)
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, CornerRadius::same(12), fill);
    }
    widgets::paint_glyph(ui.painter(), icon, rect.center(), 20.0, p.ink);
    if badge > 0 {
        let dot = Rect::from_center_size(Pos2::new(rect.right() - 10.0, rect.top() + 10.0), Vec2::splat(16.0));
        ui.painter().circle_filled(dot.center(), 8.0, p.violet);
        let label = if badge > 9 { "9+".into() } else { badge.to_string() };
        ui.painter().text(
            dot.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(9.5),
            p.on_accent,
        );
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_hud_is_noop_on_desktop() {
        let ctx = egui::Context::default();
        let p = crate::theme::Theme::default().palette();
        let mut hud = MobileHudState::default();
        let info = MobileHudContext {
            region_name: "Sandbox",
            fps: 60.0,
            unread_chat: 2,
            voice_connected: true,
            mic_on: false,
            recent_chat: vec![("Alice", "Hello!")],
        };
        let mut action = MobileHudAction::default();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0))),
                ..Default::default()
            },
            |ui| {
                action = hud.show(ui.ctx(), &p, &info);
            },
        );
        if !FormFactor::is_touch_device() {
            assert!(output.shapes.is_empty());
            assert!(!action.open_chat);
        }
    }
}
