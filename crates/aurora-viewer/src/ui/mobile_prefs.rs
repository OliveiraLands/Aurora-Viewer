//! Isolated mobile/touch ergonomic preferences persisted separately from
//! desktop `Settings` (`mobile_settings.json`) to keep upstream merges
//! conflict-free and desktop behavior 100% untouched.

use crate::theme::Palette;
use crate::ui::touch_pad::TouchPadMode;
use crate::ui::widgets;
use egui::{RichText, Vec2};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// User-configurable mobile/touch ergonomics.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MobilePrefs {
    /// Floating (appears where left thumb touches) vs Fixed bottom-left stick.
    pub floating_pad: bool,
    /// Opacity multiplier for the virtual pad overlay (`0.3..=1.0`).
    pub pad_opacity: f32,
    /// Touch camera orbit sensitivity multiplier (`0.3..=2.5`).
    pub camera_sensitivity: f32,
    /// Swap thumbstick (right) and action cluster (left) for left-handed play.
    pub left_handed: bool,
    /// Short haptic feedback on long-press, Auto-Run lock, and actions.
    pub haptics_enabled: bool,
}

impl Default for MobilePrefs {
    fn default() -> Self {
        Self {
            floating_pad: true,
            pad_opacity: 0.78,
            camera_sensitivity: 1.0,
            left_handed: false,
            haptics_enabled: true,
        }
    }
}

impl MobilePrefs {
    pub fn pad_mode(self) -> TouchPadMode {
        if self.floating_pad {
            TouchPadMode::Floating
        } else {
            TouchPadMode::Fixed
        }
    }
}

fn store() -> &'static Mutex<MobilePrefs> {
    static STORE: OnceLock<Mutex<MobilePrefs>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(load_from_disk()))
}

fn prefs_path() -> PathBuf {
    crate::settings::config_dir().join("mobile_settings.json")
}

fn load_from_disk() -> MobilePrefs {
    let Ok(bytes) = std::fs::read(prefs_path()) else {
        return MobilePrefs::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_to_disk(prefs: MobilePrefs) {
    let path = prefs_path();
    if let Ok(json) = serde_json::to_vec_pretty(&prefs) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = crate::atomic_file::write(&path, &json);
    }
}

/// Snapshot of the current mobile ergonomic settings.
pub fn current() -> MobilePrefs {
    store().lock().map(|g| *g).unwrap_or_default()
}

/// Update and persist mobile ergonomic settings.
pub fn set(prefs: MobilePrefs) {
    if let Ok(mut guard) = store().lock() {
        *guard = prefs;
    }
    save_to_disk(prefs);
}

/// Render the dedicated "Touch / Mobile Controls" settings panel inside
/// Preferences on touch devices.
pub fn show_panel(ui: &mut egui::Ui, p: &Palette) {
    let mut prefs = current();
    let before = prefs;

    ui.spacing_mut().item_spacing.y = 10.0;
    ui.label(RichText::new("Mobile & Touch Controls").size(16.0).strong().color(p.ink));
    ui.label(
        RichText::new("Ergonomic controls for phones and tablets. Desktop settings remain unchanged.")
            .size(12.5)
            .color(p.muted),
    );
    widgets::divider(ui, p);

    ui.checkbox(&mut prefs.floating_pad, "Floating virtual thumbstick (spawns where thumb touches)");
    ui.checkbox(&mut prefs.left_handed, "Left-handed layout (swap thumbstick and action buttons)");
    ui.checkbox(&mut prefs.haptics_enabled, "Haptic feedback (long-press, Auto-Run, actions)");

    ui.add_space(4.0);
    stepper_slider(ui, p, "Touch camera sensitivity", &mut prefs.camera_sensitivity, 0.3..=2.5, 0.1);
    stepper_slider(ui, p, "Virtual pad opacity", &mut prefs.pad_opacity, 0.3..=1.0, 0.05);

    ui.add_space(8.0);
    if widgets::action_button(
        ui,
        p,
        "refresh",
        "Reset mobile controls",
        widgets::ActionKind::Flat,
        Vec2::new(0.0, 38.0),
    )
    .clicked()
    {
        prefs = MobilePrefs::default();
    }

    if prefs != before {
        set(prefs);
    }
}

fn stepper_slider(ui: &mut egui::Ui, p: &Palette, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, step: f32) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(13.0).color(p.ink));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{:.2}", *value)).size(12.5).color(p.muted));
            });
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if ui.add_sized(Vec2::splat(36.0), egui::Button::new("−")).clicked() {
                *value = (*value - step).clamp(*range.start(), *range.end());
            }
            ui.add(egui::Slider::new(value, range.clone()).show_value(false));
            if ui.add_sized(Vec2::splat(36.0), egui::Button::new("+")).clicked() {
                *value = (*value + step).clamp(*range.start(), *range.end());
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_prefs_defaults_and_pad_mode_roundtrip() {
        let prefs = MobilePrefs::default();
        assert_eq!(prefs.pad_mode(), TouchPadMode::Floating);
        assert!(prefs.haptics_enabled);
        assert!(!prefs.left_handed);

        let fixed = MobilePrefs {
            floating_pad: false,
            ..prefs
        };
        assert_eq!(fixed.pad_mode(), TouchPadMode::Fixed);
        let json = serde_json::to_string(&fixed).expect("serialize");
        let parsed: MobilePrefs = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, fixed);
    }
}
