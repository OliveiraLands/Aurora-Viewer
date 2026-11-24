//! Mobile-only Bottom Action Sheet for world context menus (long-press on
//! objects, avatars, attachments, or ground).
//!
//! Strictly isolated for touch devices (`FormFactor::is_touch_device()`):
//! desktop context menus (`menu::popup_at` and `pie::show`) remain 100%
//! unchanged.

use super::entries::{Entry, Kind};
use super::{ContextMenu, Target, request};
use crate::theme::Palette;
use crate::touch::{FormFactor, ResponsiveLayout};
use crate::ui::widgets;
use egui::{Color32, CornerRadius, Id, Pos2, Rect, RichText, Sense, Stroke, Vec2};

const ACTION_ROW_H: f32 = 48.0;
const HEADER_H: f32 = 44.0;

/// Compute the sheet rect for the mobile context Action Sheet inside `usable_rect`.
pub(crate) fn action_sheet_rect(layout: ResponsiveLayout, item_count: usize) -> Rect {
    let usable = layout.usable_rect;
    let width = match layout.form_factor {
        FormFactor::CompactPortrait => usable.width(),
        _ => usable.width().min(440.0),
    };
    let desired_h = HEADER_H + 24.0 + (item_count.max(1) as f32) * (ACTION_ROW_H + 2.0);
    let max_h = (usable.height() * 0.72).max(200.0);
    let height = desired_h.min(max_h).min(usable.height());
    let left = usable.center().x - width * 0.5;
    Rect::from_min_max(Pos2::new(left, usable.bottom() - height), Pos2::new(left + width, usable.bottom()))
}

/// Resolve the active submenu slice and its title from `menu.pie_path`.
fn resolve_level<'a>(root: &'a [Entry], path: &mut Vec<usize>, default_title: &str) -> (&'a [Entry], String) {
    let mut current = root;
    let mut title = default_title.to_owned();
    let mut valid_len = 0;
    for &idx in path.iter() {
        if let Some(entry) = current.get(idx)
            && let Kind::Submenu(sub) = &entry.kind
        {
            title = entry.label.clone();
            current = sub;
            valid_len += 1;
        } else {
            break;
        }
    }
    path.truncate(valid_len);
    (current, title)
}

fn target_title(target: &Target) -> &'static str {
    match target {
        Target::Ground => "Ground",
        Target::Object { .. } => "Object",
        Target::Attachment { .. } => "Attachment",
        Target::Avatar { own: true, .. } => "My Avatar",
        Target::Avatar { own: false, .. } => "Resident",
    }
}

/// Show a touch-first Bottom Action Sheet for `entries` and return `None` for
/// desktop hover-sound tracking.
pub(super) fn show(ctx: &egui::Context, p: &Palette, menu: &mut ContextMenu, entries: &[Entry], open: &mut bool) -> Option<usize> {
    let layout = ResponsiveLayout::from_ctx(ctx);
    let (level, title) = resolve_level(entries, &mut menu.pie_path, target_title(&menu.target));
    let visible_count = level
        .iter()
        .filter(|e| !matches!(e.kind, Kind::Separator | Kind::Unavailable))
        .count();
    let rect = action_sheet_rect(layout, visible_count);

    // Dimmed backdrop behind the Action Sheet: tap outside to dismiss.
    egui::Area::new(Id::unique("mobile_action_sheet_backdrop"))
        .order(egui::Order::Middle)
        .fixed_pos(layout.screen_rect.min)
        .show(ctx, |ui| {
            let (bg_rect, resp) = ui.allocate_exact_size(layout.screen_rect.size(), Sense::click());
            ui.painter()
                .rect_filled(bg_rect, CornerRadius::ZERO, Color32::from_black_alpha(120));
            if resp.clicked() {
                *open = false;
            }
        });

    let frame = egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(p.panel.r(), p.panel.g(), p.panel.b(), 248))
        .stroke(Stroke::new(1.0, p.muted_dim.gamma_multiply(0.5)))
        .corner_radius(CornerRadius {
            nw: 16,
            ne: 16,
            sw: 0,
            se: 0,
        })
        .inner_margin(egui::Margin::symmetric(12, 8));

    let mut push_submenu = None;
    let mut pop_submenu = false;

    egui::Area::new(Id::unique("mobile_action_sheet"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .movable(false)
        .show(ctx, |ui| {
            frame.show(ui, |ui| {
                let inner_w = (rect.width() - 24.0).max(180.0);
                ui.set_min_width(inner_w);
                ui.set_max_width(inner_w);

                // Header with optional Back button (for submenus) and Close button.
                ui.allocate_ui_with_layout(
                    Vec2::new(inner_w, HEADER_H),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if !menu.pie_path.is_empty() && sheet_header_btn(ui, p, "arrow-left").clicked() {
                            pop_submenu = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if sheet_header_btn(ui, p, "x").clicked() {
                                *open = false;
                            }
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.add(egui::Label::new(RichText::new(&title).size(15.0).strong().color(p.ink)).truncate());
                            });
                        });
                    },
                );

                widgets::divider(ui, p);
                ui.add_space(4.0);

                let list_h = (rect.height() - HEADER_H - 28.0).max(80.0);
                egui::ScrollArea::vertical()
                    .id_salt(("mobile_action_sheet_rows", menu.pie_path.len()))
                    .max_height(list_h)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for (idx, entry) in level.iter().enumerate() {
                            match &entry.kind {
                                Kind::Separator => {
                                    ui.add_space(2.0);
                                    widgets::divider(ui, p);
                                    ui.add_space(2.0);
                                }
                                Kind::Unavailable => {}
                                Kind::Info => {
                                    ui.add_space(2.0);
                                    ui.label(RichText::new(&entry.label).size(12.0).color(p.muted));
                                    ui.add_space(2.0);
                                }
                                Kind::Inactive => {
                                    action_row(ui, p, inner_w, entry, false, false);
                                }
                                Kind::Submenu(_) => {
                                    if action_row(ui, p, inner_w, entry, entry.enabled, true).clicked() && entry.enabled {
                                        push_submenu = Some(idx);
                                    }
                                }
                                Kind::Action(act) => {
                                    if action_row(ui, p, inner_w, entry, entry.enabled, false).clicked() && entry.enabled {
                                        request(ctx, act.clone());
                                        if !entry.keeps_open() {
                                            *open = false;
                                        }
                                    }
                                }
                            }
                        }
                    });
            });
        });

    if pop_submenu {
        menu.pie_path.pop();
    } else if let Some(idx) = push_submenu {
        menu.pie_path.push(idx);
    }
    None
}

fn sheet_header_btn(ui: &mut egui::Ui, p: &Palette, icon: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::click());
    let fill = if resp.is_pointer_button_down_on() {
        p.raised
    } else {
        p.bar.gamma_multiply(0.6)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(10), fill);
    widgets::paint_glyph(ui.painter(), icon, rect.center(), 18.0, p.ink);
    resp
}

fn action_row(ui: &mut egui::Ui, p: &Palette, width: f32, entry: &Entry, enabled: bool, has_submenu: bool) -> egui::Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, ACTION_ROW_H), sense);
    if enabled && (resp.is_pointer_button_down_on() || resp.hovered()) {
        ui.painter().rect_filled(rect, CornerRadius::same(10), p.raised.gamma_multiply(0.8));
    }
    let ink = if enabled { p.ink } else { p.muted_dim };
    let icon_tint = if enabled { p.violet_light } else { p.muted_dim };
    if !entry.icon.is_empty() {
        widgets::paint_glyph(
            ui.painter(),
            entry.icon,
            Pos2::new(rect.left() + 20.0, rect.center().y),
            19.0,
            icon_tint,
        );
    }
    let text_left = rect.left() + 44.0;
    let right_pad = if has_submenu || entry.checked.is_some() { 34.0 } else { 12.0 };
    widgets::paint_elided(
        ui,
        &entry.label,
        Pos2::new(text_left, rect.center().y),
        (rect.width() - 44.0 - right_pad).max(40.0),
        egui::FontId::proportional(14.0),
        ink,
    );
    if entry.checked == Some(true) {
        widgets::paint_glyph(
            ui.painter(),
            "check",
            Pos2::new(rect.right() - 18.0, rect.center().y),
            16.0,
            p.violet_light,
        );
    } else if has_submenu {
        widgets::paint_glyph(
            ui.painter(),
            "chevron-right",
            Pos2::new(rect.right() - 18.0, rect.center().y),
            16.0,
            p.muted,
        );
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::touch::SafeInsets;

    #[test]
    fn action_sheet_rect_fits_bottom_of_usable_area() {
        let portrait = ResponsiveLayout::compute(
            Rect::from_min_size(Pos2::ZERO, Vec2::new(390.0, 844.0)),
            SafeInsets {
                top: 44.0,
                bottom: 20.0,
                left: 0.0,
                right: 0.0,
            },
            0.0,
        );
        let r = action_sheet_rect(portrait, 6);
        assert_eq!(r.width(), 390.0);
        assert_eq!(r.bottom(), portrait.usable_rect.bottom());
        assert!(r.height() <= portrait.usable_rect.height() * 0.72 + 1.0);

        let landscape = ResponsiveLayout::compute(Rect::from_min_size(Pos2::ZERO, Vec2::new(844.0, 390.0)), SafeInsets::ZERO, 0.0);
        let lr = action_sheet_rect(landscape, 6);
        assert!(lr.width() <= 440.0);
        assert_eq!(lr.bottom(), landscape.usable_rect.bottom());
    }

    #[test]
    fn resolve_level_navigates_submenus_and_clamps_invalid_paths() {
        let root = vec![Entry {
            icon: "dots",
            label: "More".into(),
            enabled: true,
            checked: None,
            kind: Kind::Submenu(vec![Entry {
                icon: "info-circle",
                label: "Inspect".into(),
                enabled: true,
                checked: None,
                kind: Kind::Inactive,
            }]),
        }];
        let mut path = vec![0];
        let (sub, title) = resolve_level(&root, &mut path, "Object");
        assert_eq!(title, "More");
        assert_eq!(sub.len(), 1);
        assert_eq!(sub[0].label, "Inspect");

        let mut bad = vec![99];
        let (fallback, title) = resolve_level(&root, &mut bad, "Object");
        assert_eq!(title, "Object");
        assert_eq!(fallback.len(), 1);
        assert!(bad.is_empty());
    }
}
