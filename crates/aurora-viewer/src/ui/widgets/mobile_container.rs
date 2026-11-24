//! Mobile-only responsive containers for `Floater` and `Modal`.
//!
//! Isolated from desktop `widgets.rs` / `workspace.rs` so desktop behavior
//! remains 100% identical to upstream Aurora Viewer and upstream merges stay
//! conflict-free.
//!
//! - `CompactPortrait`: Bottom Sheet / Full-Screen Sheet with top drag handle,
//!   44-48dp touch header, and single-active-panel management.
//! - `CompactLandscape`: Right-docked Side Drawer (~54% width) preserving the
//!   3D world view on the left.
//! - `Modal`: Centered inside `usable_rect` (above the virtual keyboard) with
//!   fluid width clamping.

use super::{FLOATER_STROKE, Floater, TITLE_SIZE, border_stroke, close_asked, divider, glyph, workspace};
use crate::theme::Palette;
use crate::touch::{FormFactor, ResponsiveLayout};
use egui::{Color32, CornerRadius, Id, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::collections::HashSet;

const SHEET_HEADER_H: f32 = 44.0;
const DRAG_HANDLE_H: f32 = 20.0;
const CLOSE_BTN_SIZE: f32 = 44.0;
const ACTION_BTN_SIZE: f32 = 38.0;
const DISMISS_DRAG_THRESHOLD: f32 = 80.0;
const EXPAND_DRAG_THRESHOLD: f32 = 40.0;

#[derive(Clone, Default)]
struct MobileSheetStack {
    frame: u64,
    active: Option<Id>,
    prev_open: HashSet<Id>,
    curr_open: HashSet<Id>,
    close_active: bool,
}

fn stack_id() -> Id {
    Id::unique("mobile_sheet_stack")
}

fn begin_frame_if_needed(stack: &mut MobileSheetStack, frame: u64) {
    if stack.frame != frame {
        stack.frame = frame;
        std::mem::swap(&mut stack.prev_open, &mut stack.curr_open);
        stack.curr_open.clear();
        if let Some(active) = stack.active
            && !stack.prev_open.contains(&active)
        {
            stack.active = None;
            stack.close_active = false;
        }
    }
}

/// Returns `true` if a mobile phone sheet/drawer is currently open.
#[allow(dead_code)]
pub(crate) fn has_active_sheet(ctx: &egui::Context) -> bool {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|d| {
        let stack = d.get_temp_mut_or_default::<MobileSheetStack>(stack_id());
        begin_frame_if_needed(stack, frame);
        stack.active.is_some()
    })
}

/// Requests closing the currently active mobile phone sheet/drawer (e.g. Android Back button).
#[allow(dead_code)]
pub(crate) fn request_close_active(ctx: &egui::Context) {
    ctx.data_mut(|d| {
        let stack = d.get_temp_mut_or_default::<MobileSheetStack>(stack_id());
        if stack.active.is_some() {
            stack.close_active = true;
        }
    });
}

/// Pure geometry calculation for a mobile sheet (`CompactPortrait`) or side
/// drawer (`CompactLandscape`) inside `usable_rect`.
pub(crate) fn sheet_rect(layout: ResponsiveLayout, expanded: bool) -> Rect {
    let usable = layout.usable_rect;
    match layout.form_factor {
        FormFactor::CompactPortrait => {
            let ratio = if layout.ime_visible() {
                0.96
            } else if expanded {
                0.92
            } else {
                0.62
            };
            let height = (usable.height() * ratio).max(280.0_f32.min(usable.height())).min(usable.height());
            Rect::from_min_max(
                Pos2::new(usable.left(), usable.bottom() - height),
                Pos2::new(usable.right(), usable.bottom()),
            )
        }
        FormFactor::CompactLandscape => {
            let width = (usable.width() * 0.54).clamp(300.0, 520.0).min(usable.width());
            Rect::from_min_max(
                Pos2::new(usable.right() - width, usable.top()),
                Pos2::new(usable.right(), usable.bottom()),
            )
        }
        FormFactor::Medium | FormFactor::Expanded => {
            let width = (usable.width() * 0.48).clamp(360.0, 520.0).min(usable.width());
            Rect::from_min_max(
                Pos2::new(usable.right() - width, usable.top()),
                Pos2::new(usable.right(), usable.bottom()),
            )
        }
    }
}

/// Pure geometry calculation for a responsive mobile modal: returns
/// `(center_offset_from_screen_center, max_content_width)` so modals stay
/// centered inside `usable_rect` above the virtual keyboard.
pub(crate) fn modal_layout(layout: ResponsiveLayout) -> (Vec2, f32) {
    let offset = layout.usable_rect.center() - layout.screen_rect.center();
    let max_width = (layout.usable_rect.width() - 28.0).clamp(240.0, 480.0);
    (offset, max_width)
}

fn claim_active_slot(ctx: &egui::Context, window_id: Id) -> (bool, bool) {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|d| {
        let stack = d.get_temp_mut_or_default::<MobileSheetStack>(stack_id());
        begin_frame_if_needed(stack, frame);
        let newly_opened = !stack.prev_open.contains(&window_id);
        if newly_opened || stack.active.is_none() {
            stack.active = Some(window_id);
            stack.close_active = false;
        }
        if stack.active == Some(window_id) {
            stack.curr_open.insert(window_id);
            let close_now = std::mem::take(&mut stack.close_active);
            (true, close_now)
        } else {
            (false, false)
        }
    })
}

fn release_active_slot(ctx: &egui::Context, window_id: Id) {
    ctx.data_mut(|d| {
        let stack = d.get_temp_mut_or_default::<MobileSheetStack>(stack_id());
        if stack.active == Some(window_id) {
            stack.active = None;
            stack.close_active = false;
        }
        stack.curr_open.remove(&window_id);
        stack.prev_open.remove(&window_id);
    });
}

/// Render a `Floater` as a touch-first Bottom Sheet (`CompactPortrait`) or
/// Side Drawer (`CompactLandscape`).
pub(crate) fn show_floater<R>(
    mut floater: Floater<'_>,
    ctx: &egui::Context,
    p: &Palette,
    open: &mut bool,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    let window_id = Id::unique(floater.id);
    let (is_active, back_close) = claim_active_slot(ctx, window_id);
    if !is_active {
        *open = false;
        return None;
    }

    if !floater.silent {
        super::super::sound_cues::floater_shown(ctx, window_id);
    }
    let mut close = back_close || super::super::close_key::take(ctx, window_id) || close_asked(ctx, floater.id);
    super::super::close_key::shown(ctx, window_id);

    let layout = ResponsiveLayout::from_ctx(ctx);
    let fullscreen = floater.id == crate::ui::options::WINDOW;
    let expanded_id = window_id.with("mobile_sheet_expanded");
    let mut expanded = ctx.data(|d| d.get_temp::<bool>(expanded_id)).unwrap_or(false);
    let rect = if fullscreen {
        layout.usable_rect
    } else {
        sheet_rect(layout, expanded)
    };
    let is_portrait = layout.form_factor == FormFactor::CompactPortrait;

    let corner_radius = if fullscreen {
        CornerRadius::ZERO
    } else if is_portrait {
        CornerRadius {
            nw: 16,
            ne: 16,
            sw: 0,
            se: 0,
        }
    } else {
        CornerRadius {
            nw: 14,
            ne: 0,
            sw: 14,
            se: 0,
        }
    };

    let opacity = super::super::surface_opacity::window(ctx, floater.id, floater.chat);
    let stroke = if fullscreen {
        Stroke::NONE
    } else {
        border_stroke(p, Stroke::new(FLOATER_STROKE, p.muted_dim.gamma_multiply(0.55)))
    };
    let frame = egui::Frame::new()
        .fill(super::super::surface_opacity::frame_fill(
            p,
            Color32::from_rgba_unmultiplied(p.panel.r(), p.panel.g(), p.panel.b(), p.panel_alpha.max(235)),
            opacity,
        ))
        .stroke(stroke)
        .corner_radius(corner_radius)
        .inner_margin(egui::Margin {
            left: if fullscreen { 8 } else { 10 },
            right: if fullscreen { 8 } else { 10 },
            top: if is_portrait || fullscreen { 4 } else { 8 },
            bottom: 8,
        });

    let mut out = None;
    let mut glass_slot = None;
    let response = egui::Area::new(window_id)
        .order(floater.order)
        .fixed_pos(rect.min)
        .movable(false)
        .show(ctx, |ui| {
            frame.show(ui, |ui| {
                glass_slot = Some((ui.painter().clone(), ui.painter().add(egui::Shape::Noop)));
                let inner_w = (rect.width() - 20.0).max(160.0);
                let inner_h = (rect.height() - 16.0).max(120.0);
                ui.set_min_size(Vec2::new(inner_w, inner_h));
                ui.set_max_size(Vec2::new(inner_w, inner_h));

                // Top drag handle pill in portrait Bottom Sheet mode (hidden for full-screen sheets).
                if is_portrait && !fullscreen {
                    let (handle_rect, handle_resp) = ui.allocate_exact_size(Vec2::new(inner_w, DRAG_HANDLE_H), Sense::click_and_drag());
                    let pill = Rect::from_center_size(handle_rect.center(), Vec2::new(40.0, 4.0));
                    ui.painter()
                        .rect_filled(pill, CornerRadius::same(2), p.muted_dim.gamma_multiply(0.75));
                    if handle_resp.clicked() {
                        expanded = !expanded;
                    }
                    if handle_resp.drag_stopped() {
                        let dy = ui.input(|i| i.pointer.total_drag_delta()).map_or(0.0, |d| d.y);
                        if dy < -EXPAND_DRAG_THRESHOLD {
                            expanded = true;
                        } else if dy > DISMISS_DRAG_THRESHOLD && !expanded {
                            close = true;
                        } else if dy > EXPAND_DRAG_THRESHOLD && expanded {
                            expanded = false;
                        }
                    }
                }

                // Touch-friendly header row (44dp height, large close button).
                let header_resp = ui
                    .allocate_ui_with_layout(
                        Vec2::new(inner_w, SHEET_HEADER_H),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            let icon = floater.framed_header.unwrap_or_else(|| workspace::window_icon(floater.id));
                            glyph(ui, icon, p.violet_light, 19.0);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                if sheet_icon_button(ui, p, "x", CLOSE_BTN_SIZE)
                                    .on_hover_text(aurora_i18n::tr!("common-close"))
                                    .clicked()
                                {
                                    close = true;
                                }
                                for (act_icon, tip, clicked) in &mut floater.actions {
                                    if sheet_icon_button(ui, p, act_icon, ACTION_BTN_SIZE).on_hover_text(*tip).clicked() {
                                        **clicked = true;
                                    }
                                }
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    ui.add(
                                        egui::Label::new(RichText::new(&floater.title).size(TITLE_SIZE + 1.0).strong().color(p.ink))
                                            .truncate(),
                                    );
                                    if let Some(note) = &floater.title_note {
                                        ui.add(egui::Label::new(RichText::new(note).size(12.0).color(p.muted)).truncate());
                                    }
                                });
                            });
                        },
                    )
                    .response;

                // Swipe-right on landscape drawer header to dismiss.
                if !is_portrait {
                    let drag = ui.interact(header_resp.rect, window_id.with("drawer_drag"), Sense::drag());
                    if drag.drag_stopped() {
                        let dx = ui.input(|i| i.pointer.total_drag_delta()).map_or(0.0, |d| d.x);
                        if dx > DISMISS_DRAG_THRESHOLD {
                            close = true;
                        }
                    }
                }

                divider(ui, p);
                ui.add_space(4.0);

                // Gentle hint on very narrow portrait screens for complex desktop-class tools.
                if is_portrait && matches!(floater.id, "director" | "dev_toolbox" | "build_tools") {
                    egui::Frame::new()
                        .fill(p.violet.gamma_multiply(0.14))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(8, 5))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new("Tip: rotate to landscape or use a tablet for more workspace.")
                                    .size(11.5)
                                    .color(p.violet_pale),
                            );
                        });
                    ui.add_space(4.0);
                }

                // Scrollable body filling the remaining sheet height.
                let body_h = ui.available_height().max(80.0);
                let scroll = egui::ScrollArea::vertical()
                    .id_salt((floater.id, "mobile_sheet_body"))
                    .max_height(body_h)
                    .min_scrolled_height(body_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        crate::ui::ime_composer::keep_focused_visible(ui);
                        body(ui)
                    });
                out = Some(scroll.inner);
            });
        });

    if let Some((painter, slot)) = glass_slot {
        let radius = f32::from(corner_radius.nw);
        super::super::surface_opacity::fade_after(ctx, painter.layer_id(), slot, p, opacity);
        workspace::finish_glass_faded(
            &painter.with_clip_rect(response.response.rect),
            slot,
            response.response.rect,
            p,
            radius,
            opacity,
        );
    }

    ctx.data_mut(|d| d.insert_temp(expanded_id, expanded));

    if close {
        *open = false;
        release_active_slot(ctx, window_id);
        if !floater.silent {
            super::super::sound_cues::request(ctx, crate::ui_sound::UiSound::WindowClose);
        }
    }

    out
}

fn sheet_icon_button(ui: &mut egui::Ui, p: &Palette, icon: &str, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let fill = if resp.is_pointer_button_down_on() {
        p.raised
    } else if resp.hovered() {
        p.raised.gamma_multiply(0.7)
    } else {
        p.bar.gamma_multiply(0.55)
    };
    ui.painter().rect_filled(rect, CornerRadius::same((size * 0.25) as u8), fill);
    workspace::paint_glyph(
        ui.painter(),
        icon,
        rect.center(),
        18.0,
        if resp.hovered() { p.ink } else { p.muted },
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::touch::SafeInsets;

    #[test]
    fn portrait_sheet_uses_bottom_detents_and_expands_with_ime() {
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(390.0, 844.0));
        let insets = SafeInsets {
            top: 44.0,
            bottom: 20.0,
            left: 0.0,
            right: 0.0,
        };
        let layout = ResponsiveLayout::compute(screen, insets, 0.0);
        let half = sheet_rect(layout, false);
        let full = sheet_rect(layout, true);
        assert_eq!(half.width(), 390.0);
        assert_eq!(half.bottom(), layout.usable_rect.bottom());
        assert!(full.height() > half.height());

        let ime_layout = ResponsiveLayout::compute(screen, insets, 300.0);
        let ime_sheet = sheet_rect(ime_layout, false);
        assert_eq!(ime_sheet.bottom(), ime_layout.usable_rect.bottom());
        assert!(ime_sheet.height() >= ime_layout.usable_rect.height() * 0.95);
    }

    #[test]
    fn landscape_drawer_docks_right_and_preserves_world_view() {
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(844.0, 390.0));
        let layout = ResponsiveLayout::compute(screen, SafeInsets::ZERO, 0.0);
        assert_eq!(layout.form_factor, FormFactor::CompactLandscape);
        let drawer = sheet_rect(layout, false);
        assert_eq!(drawer.right(), 844.0);
        assert_eq!(drawer.height(), 390.0);
        assert!(drawer.left() > 300.0, "left side stays open for 3D world");
    }

    #[test]
    fn modal_layout_shifts_above_ime_and_clamps_width() {
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(390.0, 844.0));
        let layout = ResponsiveLayout::compute(screen, SafeInsets::ZERO, 300.0);
        let (offset, max_w) = modal_layout(layout);
        assert!(offset.y < -100.0, "modal shifts upward above keyboard");
        assert!((240.0..=362.0).contains(&max_w));
    }

    #[test]
    fn single_active_sheet_replaces_previous_panel_on_new_open() {
        let ctx = egui::Context::default();
        let a = Id::unique("sheet_a");
        let b = Id::unique("sheet_b");

        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert_eq!(claim_active_slot(ui.ctx(), a), (true, false));
        });
        assert!(has_active_sheet(&ctx));

        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            // Frame 2: B opens after A was already open.
            assert_eq!(claim_active_slot(ui.ctx(), a), (true, false));
            assert_eq!(claim_active_slot(ui.ctx(), b), (true, false));
        });

        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            // Frame 3: A is no longer active; B owns the slot.
            assert_eq!(claim_active_slot(ui.ctx(), a), (false, false));
            assert_eq!(claim_active_slot(ui.ctx(), b), (true, false));
        });

        request_close_active(&ctx);
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert_eq!(claim_active_slot(ui.ctx(), b), (true, true));
            release_active_slot(ui.ctx(), b);
        });
        assert!(!has_active_sheet(&ctx));
    }

    #[test]
    fn sheet_clears_automatically_when_no_longer_rendered() {
        let ctx = egui::Context::default();
        let a = Id::unique("auto_closed_sheet");

        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert_eq!(claim_active_slot(ui.ctx(), a), (true, false));
        });
        assert!(has_active_sheet(&ctx));

        // Frame where no sheet calls `claim_active_slot` (panel closed externally).
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let _ = has_active_sheet(ui.ctx());
        });
        // Next frame observes `prev_open` is empty and clears `active`.
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(!has_active_sheet(ui.ctx()));
        });
    }
}
