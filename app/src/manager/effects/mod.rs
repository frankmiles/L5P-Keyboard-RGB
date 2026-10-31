use default_ui::{show_brightness, show_direction, show_effect_settings};
use eframe::egui::{self, ComboBox, Slider};
use strum::IntoEnumIterator;

use crate::{
    enums::{AudioSource, AudioVisualStyle, Effects, SwipeMode, TypingStyle},
    manager::profile::Profile,
};

pub mod ambient;
pub mod audio;
pub mod christmas;
pub mod default_ui;
pub mod disco;
pub mod expanded;
pub mod fade;
pub mod lightning;
pub mod ripple;
pub mod swipe;
pub mod temperature;
pub mod zones;

pub fn show_effect_ui(ui: &mut egui::Ui, profile: &mut Profile, update_lights: &mut bool, theme: &crate::gui::style::Theme) {
    let mut effect = profile.effect;

    match &mut effect {
        Effects::SmoothWave { mode, clean_with_black } | Effects::Swipe { mode, clean_with_black } => {
            ui.scope(|ui| {
                ui.style_mut().spacing.item_spacing = theme.spacing.default;

                show_brightness(ui, profile, update_lights);
                show_direction(ui, profile, update_lights);
                show_effect_settings(ui, profile, update_lights);
                ComboBox::from_label("Swipe mode").width(30.0).selected_text(format!("{:?}", mode)).show_ui(ui, |ui| {
                    for swipe_mode in SwipeMode::iter() {
                        *update_lights |= ui.selectable_value(mode, swipe_mode, format!("{:?}", swipe_mode)).changed();
                    }
                });
                *update_lights |= ui.add_enabled(matches!(mode, SwipeMode::Fill), egui::Checkbox::new(clean_with_black, "Clean with black")).changed();
            });
        }
        Effects::AmbientLight { fps, saturation_boost } => {
            ui.scope(|ui| {
                ui.style_mut().spacing.item_spacing = theme.spacing.default;

                show_brightness(ui, profile, update_lights);
                show_direction(ui, profile, update_lights);

                ui.horizontal(|ui| {
                    *update_lights |= ui.add(Slider::new(fps, 1..=60)).changed();
                    ui.label("FPS");
                });
                ui.horizontal(|ui| {
                    *update_lights |= ui.add(Slider::new(saturation_boost, 0.0..=1.0)).changed();
                    ui.label("Saturation Boost");
                });
            });
        }
        Effects::AudioVisualizer { source, style } => {
            ui.scope(|ui| {
                ui.style_mut().spacing.item_spacing = theme.spacing.default;

                show_brightness(ui, profile, update_lights);
                show_effect_settings(ui, profile, update_lights);
                ComboBox::from_label("Audio input").width(100.0).selected_text(source.to_string()).show_ui(ui, |ui| {
                    for val in AudioSource::iter() {
                        *update_lights |= ui.selectable_value(source, val, val.to_string()).changed();
                    }
                });
                ComboBox::from_label("Visualizer").width(100.0).selected_text(style.to_string()).show_ui(ui, |ui| {
                    for val in AudioVisualStyle::iter() {
                        *update_lights |= ui.selectable_value(style, val, val.to_string()).changed();
                    }
                });
                ui.label("Analyzes audio in memory only; nothing is recorded.");
            });
        }
        Effects::TypingStudio { style } => {
            ui.scope(|ui| {
                ui.style_mut().spacing.item_spacing = theme.spacing.default;

                show_brightness(ui, profile, update_lights);
                show_direction(ui, profile, update_lights);
                show_effect_settings(ui, profile, update_lights);
                ComboBox::from_label("Typing style").width(100.0).selected_text(style.to_string()).show_ui(ui, |ui| {
                    for val in TypingStyle::iter() {
                        *update_lights |= ui.selectable_value(style, val, val.to_string()).changed();
                    }
                });
            });
        }
        _ => {
            default_ui::show(ui, profile, update_lights, &theme.spacing);
        }
    }

    profile.effect = effect;
}
