//! Shared login information, community links, version footer and the
//! preferences button.
use super::{LoginAction, LoginForm};
use crate::theme::Palette;
use crate::ui::{
    icons::Icons,
    news::LoginInfo,
    widgets::{self, ActionKind},
};
use egui::{Context, Id, Rect, RichText, Vec2};

const ACTION_TOP: f32 = 20.0;
const ACTION_SIZE: f32 = 30.0;

pub fn sidebar(ctx: &Context, p: &Palette, icons: &Icons, info: &LoginInfo, card: Rect, width: f32, lowest: f32) -> bool {
    let screen = ctx.content_rect();
    let left = screen.right() - 32.0 - width;
    // Both login layouts share the top-right actions. Give their row a gutter
    // even when a short window pushes the version panel down onto the news.
    let column = Rect::from_min_max(
        egui::pos2(left, screen.top() + ACTION_TOP + ACTION_SIZE + 8.0),
        egui::pos2(screen.right(), lowest),
    );
    let version = egui::Area::new(Id::unique("login_version"))
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(egui::pos2(left, card.top() - 14.0))
        .constrain_to(column)
        .show(ctx, |ui| {
            ui.set_width(width);
            super::version_panel(ui, p, icons, info)
        });
    let news_top = card.top().max(version.response.rect.bottom() + 14.0);
    egui::Area::new(Id::unique("login_news"))
        .fixed_pos(egui::pos2(left, news_top))
        .constrain_to(column)
        .show(ctx, |ui| {
            ui.set_width(width);
            super::news_panel(ui, p, icons, info, (card.bottom().min(lowest) - news_top).max(120.0));
        });
    version.inner
}

fn version(ui: &mut egui::Ui, p: &Palette) {
    let label = format!("{} v{}", crate::brand::APP_NAME, aurora_version::display_version());
    ui.label(RichText::new(label).size(11.5).color(p.muted_dim));
}

pub fn footer(ctx: &Context, p: &Palette, icons: &Icons, info: &LoginInfo, sidebar_visible: bool, form: &mut LoginForm) -> LoginAction {
    let mut action = LoginAction::None;
    let screen = ctx.content_rect();
    if form.safe_graphics.is_some() || form.photo_quality_off || form.camera_limits_restored || form.driver_notice.is_some() {
        super::driver_notice::column(ctx, |ui, width| {
            if let Some(state) = form.safe_graphics
                && let Some(choice) = super::safe_graphics_notice::card(ui, p, icons, state, width)
            {
                action = LoginAction::SafeGraphics(choice);
            }
            if form.photo_quality_off
                && let Some(back_on) = super::photo_quality_notice::card(ui, p, icons, width)
            {
                action = LoginAction::PhotoQualityOff(back_on);
            }
            if form.camera_limits_restored
                && let Some(open) = super::camera_limits_notice::card(ui, p, icons, width)
            {
                action = LoginAction::CameraLimits(open);
            }
            if let Some(notice) = &form.driver_notice
                && super::driver_notice::card(ui, p, icons, notice, width)
            {
                action = LoginAction::DismissDriverNotice;
            }
        });
    }
    let phone = crate::touch::FormFactor::is_touch_device() && crate::touch::ResponsiveLayout::from_ctx(ctx).form_factor.is_phone();
    let edge_x = if phone { 12.0 } else { 25.0 };
    let edge_y = if phone { 12.0 } else { 20.0 };
    let btn_h = if phone { 26.0 } else { 30.0 };
    egui::Area::new(Id::unique("login_links"))
        .anchor(egui::Align2::LEFT_BOTTOM, Vec2::new(edge_x, -edge_y))
        .show(ctx, |ui| {
            if !phone {
                ui.label(
                    RichText::new(aurora_i18n::tr!("ui-login-join-the-community"))
                        .size(11.5)
                        .color(p.muted),
                );
                ui.add_space(4.0);
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = if phone { 4.0 } else { 6.0 };
                if phone {
                    footer_icon_link(ui, p, "brand-discord-filled", "Discord", crate::links::DISCORD, btn_h);
                    footer_icon_link(ui, p, "brand-github-filled", "GitHub", crate::links::GITHUB, btn_h);
                    footer_icon_link(
                        ui,
                        p,
                        "world-filled",
                        aurora_i18n::tr!("ui-login-website"),
                        crate::links::WEBSITE,
                        btn_h,
                    );
                } else {
                    super::link_button(ui, p, "brand-discord-filled", "Discord", crate::links::DISCORD);
                    super::link_button(ui, p, "brand-github-filled", "GitHub", crate::links::GITHUB);
                    super::link_button(ui, p, "world-filled", aurora_i18n::tr!("ui-login-website"), crate::links::WEBSITE);
                }
                let bug_label = if phone { "" } else { aurora_i18n::tr!("bug-title") };
                let bug_size = if phone { Vec2::splat(btn_h) } else { Vec2::new(0.0, btn_h) };
                let bug = widgets::action_button(ui, p, "bug-filled", bug_label, ActionKind::Caution, bug_size)
                    .on_hover_text(aurora_i18n::tr!("bug-title"));
                if bug.clicked() {
                    action = LoginAction::ReportBug;
                } else if bug.secondary_clicked() {
                    action = LoginAction::CopyVersion;
                }
                let news_label = if phone { "" } else { aurora_i18n::tr!("ui-login-news") };
                let news_size = if phone { Vec2::splat(btn_h) } else { Vec2::new(0.0, btn_h) };
                if !sidebar_visible
                    && widgets::action_button(ui, p, "article-filled", news_label, ActionKind::Flat, news_size)
                        .on_hover_text(aurora_i18n::tr!("ui-login-news"))
                        .clicked()
                {
                    form.show_info = true;
                }
            });
        });
    if screen.width() < 1100.0 {
        let ver_y = if phone { -44.0 } else { -59.0 };
        egui::Area::new(Id::unique("login_footer_version"))
            .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-edge_x, ver_y))
            .show(ctx, |ui| version(ui, p));
    }
    // The preferences before login (Firestorm opens them from the menu bar of
    // its login screen, menu_login.xml): one cogwheel in the top right corner.
    egui::Area::new(Id::unique("login_settings"))
        .anchor(egui::Align2::RIGHT_TOP, Vec2::new(-25.0, ACTION_TOP))
        .show(ctx, |ui| {
            if widgets::action_button(ui, p, "settings-filled", "", ActionKind::Flat, Vec2::splat(ACTION_SIZE))
                .on_hover_text(aurora_i18n::tr!("common-preferences"))
                .clicked()
            {
                action = LoginAction::Preferences;
            }
        });
    // Left of it, the way to a larger, more readable interface before the
    // first login: Preferences › Accessibility.
    egui::Area::new(Id::unique("login_accessibility"))
        .anchor(egui::Align2::RIGHT_TOP, Vec2::new(-63.0, ACTION_TOP))
        .show(ctx, |ui| {
            if widgets::action_button(ui, p, "accessible-filled", "", ActionKind::Flat, Vec2::splat(ACTION_SIZE))
                .on_hover_text(aurora_i18n::tr!("login-accessibility"))
                .clicked()
            {
                action = LoginAction::Accessibility;
            }
        });
    egui::Area::new(Id::unique("login_quit"))
        .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-edge_x, -edge_y))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                if screen.width() >= 1100.0 {
                    version(ui, p);
                }
                let quit_label = if phone { "" } else { aurora_i18n::tr!("common-quit") };
                let quit_size = if phone { Vec2::splat(btn_h) } else { Vec2::new(0.0, btn_h) };
                if widgets::action_button(ui, p, "power", quit_label, ActionKind::FlatDanger, quit_size)
                    .on_hover_text(aurora_i18n::tr!("common-quit"))
                .clicked()
                {
                    action = LoginAction::Quit;
                }
            });
        });
    if form.show_info {
        let response = widgets::Modal::new(Id::unique("accounts_news_dialog")).show(ctx, |ui| {
            ui.set_width(340.0);
            if super::version_panel(ui, p, icons, info) {
                action = LoginAction::CheckUpdates;
            }
            ui.add_space(8.0);
            super::news_panel(ui, p, icons, info, 235.0);
            if widgets::default_button(ui.button(aurora_i18n::tr!("common-close"))).clicked() {
                form.show_info = false;
            }
        });
        if response.should_close() {
            form.show_info = false;
        }
    }
    action
}

fn footer_icon_link(ui: &mut egui::Ui, p: &Palette, icon: &str, name: &str, url: &str, size: f32) {
    let enabled = !url.is_empty();
    let resp = ui
        .add_enabled_ui(enabled, |ui| {
            widgets::action_button(ui, p, icon, "", ActionKind::Flat, Vec2::splat(size))
        })
        .inner
        .on_hover_text(url)
        .on_disabled_hover_text(aurora_i18n::tr!("ui-login-pending-link", name = name.to_string()));
    if resp.clicked() && enabled {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}
