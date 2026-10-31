use std::{collections::HashSet, process, thread};

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[cfg(debug_assertions)]
use eframe::egui::style::DebugOptions;
use eframe::{
    egui::{CentralPanel, Context, CornerRadius, Frame, Layout, ScrollArea, Style, TopBottomPanel, ViewportCommand},
    emath::Align,
    epaint::{Color32, Vec2},
    CreationContext,
};

use egui_notify::Toasts;
use legion_input::{EventKind, Input, Key};
use rand::Rng;
use single_instance::SingleInstance;
use strum::IntoEnumIterator;
use tray_icon::menu::MenuEvent;

use crate::{
    background::BackgroundController,
    cli::OutputType,
    enums::Effects,
    manager::{self, custom_effect::CustomEffect, profile::Profile, show_effect_ui, EffectManager, ManagerCreationError},
    persist::Settings,
    tray::{QUIT_ID, SHOW_ID},
    DENY_HIDING,
};

use self::{menu_bar::MenuBarState, saved_items::SavedItems, style::Theme};

mod menu_bar;
mod modals;
mod saved_items;
pub mod style;

pub struct App {
    _gui_instance: SingleInstance,
    instance_not_unique: bool,
    gui_tx: crossbeam_channel::Sender<GuiMessage>,
    gui_rx: crossbeam_channel::Receiver<GuiMessage>,

    has_tray: Arc<AtomicBool>,
    visible: Arc<AtomicBool>,

    manager: Option<EffectManager>,
    background: Option<BackgroundController>,
    startup_error: Option<String>,
    input: Option<Input>,
    state_changed: bool,
    loaded_effect: LoadedEffect,
    current_profile: Profile,

    menu_bar: MenuBarState,
    saved_items: SavedItems,
    global_rgb: [u8; 3],
    theme: Theme,
    toasts: Toasts,
}

pub enum GuiMessage {
    CycleProfiles,
    Quit,
}

pub struct LoadedEffect {
    state: State,
    effect: CustomEffect,
}

impl LoadedEffect {
    pub fn default() -> Self {
        Self::none()
    }

    pub fn none() -> Self {
        Self {
            state: State::None,
            effect: CustomEffect::default(),
        }
    }

    pub fn queued(effect: CustomEffect) -> Self {
        Self { state: State::Queued, effect }
    }

    pub fn is_none(&self) -> bool {
        matches!(self.state, State::None)
    }

    pub fn is_queued(&self) -> bool {
        matches!(self.state, State::Queued)
    }

    pub fn is_playing(&self) -> bool {
        matches!(self.state, State::Playing)
    }
}

#[derive(Default)]
pub enum State {
    #[default]
    None,
    Queued,
    Playing,
}

impl App {
    pub fn new(output: OutputType, has_tray: Arc<AtomicBool>, visible: Arc<AtomicBool>) -> Self {
        let (gui_tx, gui_rx) = crossbeam_channel::unbounded::<GuiMessage>();

        let gui_instance = SingleInstance::new(concat!(env!("CARGO_PKG_NAME"), "-gui")).unwrap();
        let (manager, background, instance_not_unique, startup_error) = if !gui_instance.is_single() {
            (None, None, true, None)
        } else {
            match EffectManager::new(manager::OperationMode::Gui) {
                Ok(manager) => (Some(manager), None, false, None),
                Err(error) if &ManagerCreationError::InstanceAlreadyRunning == error.current_context() => match BackgroundController::connect_existing() {
                    Ok(Some(background)) => (None, Some(background), false, None),
                    Ok(None) => (None, None, true, None),
                    Err(error) => (None, None, true, Some(error)),
                },
                Err(_) => (None, None, false, None),
            }
        };

        let input = manager.as_ref().and_then(EffectManager::input).cloned().or_else(|| background.as_ref().and_then(|_| Input::new().ok()));
        let manager_running = manager.is_some();

        let settings: Settings = Settings::load();
        let Settings {
            mut current_profile,
            profiles,
            effects,
        } = settings;
        if let Some(background) = background.as_ref() {
            let saved_name = current_profile.name.clone();
            current_profile = background.profile().clone();
            current_profile.name = saved_name;
        }

        let gui_tx_c = gui_tx.clone();
        // Default app state
        let mut app = Self {
            _gui_instance: gui_instance,
            instance_not_unique,
            gui_tx,
            gui_rx,

            has_tray,
            visible,

            manager,
            background,
            startup_error,
            input,
            state_changed: manager_running,
            loaded_effect: LoadedEffect::default(),
            current_profile,

            menu_bar: MenuBarState::new(gui_tx_c),
            saved_items: SavedItems::new(profiles, effects),
            global_rgb: [0; 3],
            theme: Theme::default(),
            toasts: Toasts::default(),
        };

        // Update the state according to the option chosen by the user
        match output {
            OutputType::Profile(profile) => {
                app.current_profile = profile;
                app.state_changed = true;
            }
            OutputType::Custom(effect) => {
                app.loaded_effect = LoadedEffect::queued(effect);
                app.state_changed = true;
            }
            OutputType::NoArgs => {}
            OutputType::Exit => unreachable!("Exiting the app supersedes starting the GUI"),
        }

        app
    }

    pub fn init(self, cc: &CreationContext<'_>) -> Self {
        if !*DENY_HIDING {
            cc.egui_ctx.send_viewport_cmd(ViewportCommand::Visible(self.visible.load(Ordering::SeqCst)));
        }

        let egui_ctx = cc.egui_ctx.clone();
        let gui_tx = self.gui_tx.clone();
        let has_tray = self.has_tray.clone();

        std::thread::spawn(move || loop {
            if let Ok(event) = MenuEvent::receiver().recv() {
                if event.id == SHOW_ID {
                    egui_ctx.request_repaint();

                    egui_ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                    egui_ctx.send_viewport_cmd(ViewportCommand::Focus);
                } else if event.id == QUIT_ID {
                    egui_ctx.request_repaint();

                    let _ = gui_tx.send(GuiMessage::Quit);
                    has_tray.store(false, Ordering::SeqCst);
                }
            }
        });

        let ctx = cc.egui_ctx.clone();
        let gui_tx_c = self.gui_tx.clone();
        if let Some(events) = self.input.as_ref().map(Input::subscribe) {
            thread::spawn(move || {
                let mut held = HashSet::new();
                let mut active = false;

                while let Ok(event) = events.recv() {
                    match event.kind {
                        EventKind::KeyDown(key) => {
                            held.insert(key);
                        }
                        EventKind::KeyUp(key) => {
                            held.remove(&key);
                        }
                        EventKind::Reset => held.clear(),
                    }

                    let chord_pressed = held.contains(&Key::MetaLeft) && held.contains(&Key::AltRight);
                    if chord_pressed && !active {
                        let _ = gui_tx_c.send(GuiMessage::CycleProfiles);
                        ctx.request_repaint();
                    }
                    active = chord_pressed;
                }
            });
        }

        self.configure_style(&cc.egui_ctx);

        self
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(message) = self.gui_rx.try_recv() {
            match message {
                GuiMessage::CycleProfiles => self.cycle_profiles(),
                GuiMessage::Quit => self.exit_app(),
            }
        }

        // Show active toast messages
        self.toasts.show(ctx);
        if let Some(error) = self.startup_error.take() {
            self.toasts.error(error).duration(Some(std::time::Duration::from_secs(8))).closable(true);
        }

        if *DENY_HIDING && !self.visible.load(Ordering::SeqCst) {
            self.visible.store(true, Ordering::SeqCst);
            self.toasts
                .warning("Window hiding is currently not supported.\nSee https://github.com/4JX/L5P-Keyboard-RGB/issues/181")
                .duration(None);
        }

        if self.instance_not_unique && modals::unique_instance(ctx) {
            self.exit_app();
        }

        if !self.instance_not_unique && self.manager.is_none() && self.background.is_none() && modals::manager_error(ctx) {
            self.exit_app();
        }

        TopBottomPanel::top("top-panel").show(ctx, |ui| {
            self.menu_bar
                .show(ctx, ui, &mut self.current_profile, &mut self.loaded_effect, &mut self.state_changed, &mut self.toasts);
        });

        let mut apply_clicked = false;
        TopBottomPanel::bottom("apply-panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.background.is_some() {
                    ui.label("Applied effect runs until Windows shuts down.");
                } else {
                    ui.label("Apply the selected built-in effect to keep it running after closing the app.");
                }
                apply_clicked = ui
                    .add_enabled(self.loaded_effect.is_none() && cfg!(target_os = "windows"), eframe::egui::Button::new("Apply until shutdown"))
                    .on_hover_text("Starts a local background process. It does not start automatically after reboot.")
                    .clicked();
            });
        });
        if apply_clicked {
            self.apply_background_effect();
        }

        CentralPanel::default()
            .frame(Frame::new().inner_margin(self.theme.spacing.large).fill(Color32::from_gray(26)))
            .show(ctx, |ui| {
                ui.style_mut().spacing.item_spacing = Vec2::splat(self.theme.spacing.large);
                self.show_ui_elements(ctx, ui);
            });

        if self.state_changed {
            self.update_state();
        }

        self.handle_close_request(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let SavedItems { profiles, custom_effects, .. } = self.saved_items.clone();

        let mut settings = Settings::new(profiles, custom_effects, self.current_profile.clone());

        settings.save();

        self.visible.store(false, Ordering::SeqCst);

        if let Some(manager) = self.manager.take() {
            manager.shutdown();
        }
    }
}

impl App {
    fn configure_style(&self, ctx: &Context) {
        let style = Style {
            // text_styles: text_utils::default_text_styles(),
            visuals: self.theme.visuals.clone(),
            #[cfg(debug_assertions)]
            debug: DebugOptions {
                debug_on_hover: false,
                debug_on_hover_with_all_modifiers: false,
                hover_shows_next: false,
                show_expand_width: false,
                show_expand_height: false,
                show_resize: false,
                show_interactive_widgets: false,
                show_widget_hits: false,
                show_unaligned: false,
            },
            ..Style::default()
        };

        // ctx.set_fonts(text_utils::get_font_def());
        ctx.set_style(style);
    }

    fn exit_app(&mut self) {
        use eframe::App;

        self.on_exit(None);

        process::exit(0);
    }

    fn cycle_profiles(&mut self) {
        let len = self.saved_items.profiles.len();

        let current_profile_name = &self.current_profile.name;

        if let Some((i, _)) = self.saved_items.profiles.iter().enumerate().find(|(_, profile)| &profile.name == current_profile_name) {
            if i == len - 1 && len > 0 {
                self.current_profile = self.saved_items.profiles[0].clone();
            } else {
                self.current_profile = self.saved_items.profiles[i + 1].clone();
            }

            self.state_changed = true;
        }
    }

    fn show_ui_elements(&mut self, ctx: &Context, ui: &mut eframe::egui::Ui) {
        ui.with_layout(Layout::left_to_right(Align::Center).with_cross_justify(true), |ui| {
            ui.vertical(|ui| {
                let can_tweak_colors = self.current_profile.effect.takes_color_array() && self.loaded_effect.is_none();

                let res = ui.add_enabled_ui(can_tweak_colors, |ui| {
                    ui.style_mut().spacing.item_spacing = Vec2::splat(self.theme.spacing.medium);
                    let response = ui.horizontal(|ui| {
                        ui.style_mut().spacing.interact_size = Vec2::new(70.0, 50.0);

                        for i in 0..4 {
                            self.state_changed |= ui.color_edit_button_srgb(&mut self.current_profile.rgb_zones[i].rgb).changed();
                        }
                    });

                    ui.style_mut().spacing.interact_size = Vec2::new(response.response.rect.width(), 30.0);
                    ui.horizontal(|ui| {
                        if ui.color_edit_button_srgb(&mut self.global_rgb).changed() {
                            for i in 0..4 {
                                self.current_profile.rgb_zones[i].rgb = self.global_rgb;
                            }
                            self.state_changed = true;
                        }

                        if ui.small_button("Random").on_hover_text("Apply a different vivid random color to each zone").clicked() {
                            let palette = random_vivid_palette();
                            self.global_rgb = palette[0];
                            for (zone, color) in self.current_profile.rgb_zones.iter_mut().zip(palette) {
                                zone.rgb = color;
                            }
                            self.state_changed = true;
                        }
                    });

                    response.response
                });

                ui.set_width(res.inner.rect.width());

                self.show_effect_ui(ui);

                self.saved_items
                    .show(ctx, ui, &mut self.current_profile, &mut self.loaded_effect, &self.theme.spacing, &mut self.state_changed);
            });

            ui.vertical_centered_justified(|ui| {
                if self.loaded_effect.is_playing() && ui.button("Stop custom effect").clicked() {
                    self.loaded_effect.state = State::None;
                    self.state_changed = true;
                }

                Frame {
                    corner_radius: CornerRadius::same(6),
                    fill: Color32::from_gray(20),
                    ..Frame::default()
                }
                .show(ui, |ui| {
                    ui.style_mut().spacing.item_spacing = self.theme.spacing.default;
                    ScrollArea::vertical().show(ui, |ui| {
                        ui.with_layout(Layout::top_down_justified(Align::Min), |ui| {
                            for val in Effects::iter() {
                                let text: &'static str = val.into();
                                if ui.selectable_value(&mut self.current_profile.effect, val, text).clicked() {
                                    self.state_changed = true;
                                    self.loaded_effect.state = State::None;
                                }
                            }
                        });
                    });
                });
            });
        });
    }

    fn show_effect_ui(&mut self, ui: &mut eframe::egui::Ui) {
        ui.add_enabled_ui(self.loaded_effect.is_none(), |ui| {
            show_effect_ui(ui, &mut self.current_profile, &mut self.state_changed, &self.theme);
        });
    }

    fn update_state(&mut self) {
        if self.loaded_effect.is_none() {
            if let Some(Err(error)) = self.background.as_mut().map(|background| background.set_profile(&self.current_profile)) {
                self.toasts
                    .error(format!("Could not update the applied effect: {error}"))
                    .duration(Some(std::time::Duration::from_secs(8)))
                    .closable(true);
                match EffectManager::new(manager::OperationMode::Gui) {
                    Ok(mut manager) => {
                        manager.set_profile(self.current_profile.clone());
                        self.manager = Some(manager);
                        self.background = None;
                    }
                    Err(manager_error) => {
                        self.toasts
                            .error(format!("The background effect may still own the keyboard: {manager_error}"))
                            .duration(Some(std::time::Duration::from_secs(8)))
                            .closable(true);
                    }
                }
            } else if let Some(manager) = self.manager.as_mut() {
                manager.set_profile(self.current_profile.clone());
            }
        } else if self.loaded_effect.is_queued() {
            let effect = self.loaded_effect.effect.clone();
            let mut can_start_custom = true;
            if let Some(background) = self.background.as_mut() {
                if let Err(error) = background.stop() {
                    self.toasts
                        .error(format!("Could not stop the applied built-in effect: {error}"))
                        .duration(Some(std::time::Duration::from_secs(8)))
                        .closable(true);
                    can_start_custom = false;
                } else {
                    self.background = None;
                }
            }

            if can_start_custom {
                if self.manager.is_none() {
                    match EffectManager::new(manager::OperationMode::Gui) {
                        Ok(manager) => self.manager = Some(manager),
                        Err(error) => {
                            self.toasts
                                .error(format!("Could not start the custom effect: {error}"))
                                .duration(Some(std::time::Duration::from_secs(8)))
                                .closable(true);
                        }
                    }
                }
                if let Some(manager) = self.manager.as_ref() {
                    manager.custom_effect(effect);
                    self.loaded_effect.state = State::Playing;
                }
            }
        }

        self.state_changed = false;
    }

    fn apply_background_effect(&mut self) {
        let profile = self.current_profile.clone();
        if let Some(background) = self.background.as_mut() {
            match background.set_profile(&profile) {
                Ok(()) => {
                    self.toasts
                        .success("Effect is active in the background until Windows shuts down.")
                        .duration(Some(std::time::Duration::from_secs(5)))
                        .closable(true);
                }
                Err(error) => {
                    self.toasts
                        .error(format!("Could not apply the selected effect: {error}"))
                        .duration(Some(std::time::Duration::from_secs(8)))
                        .closable(true);
                }
            }
            return;
        }

        if let Some(manager) = self.manager.take() {
            manager.shutdown();
        }

        match BackgroundController::launch(&profile) {
            Ok(background) => {
                self.background = Some(background);
                self.toasts
                    .success("Effect is active in the background until Windows shuts down.")
                    .duration(Some(std::time::Duration::from_secs(5)))
                    .closable(true);
            }
            Err(error) => {
                self.toasts
                    .error(format!("Could not apply the selected effect in the background: {error}"))
                    .duration(Some(std::time::Duration::from_secs(8)))
                    .closable(true);
                match EffectManager::new(manager::OperationMode::Gui) {
                    Ok(mut manager) => {
                        manager.set_profile(profile);
                        self.manager = Some(manager);
                    }
                    Err(error) => {
                        self.toasts
                            .error(format!("Could not resume the effect in the app: {error}"))
                            .duration(Some(std::time::Duration::from_secs(8)))
                            .closable(true);
                    }
                }
            }
        }
    }

    fn handle_close_request(&mut self, ctx: &Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !*DENY_HIDING {
            if self.has_tray.load(Ordering::Relaxed) {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            } else {
                // Close normally
            }
        }
    }
}

fn random_vivid_palette() -> [[u8; 3]; 4] {
    let mut rng = rand::rng();
    let start_hue = rng.random_range(0..360);
    std::array::from_fn(|zone| hsv_to_rgb(((start_hue + zone * 90) % 360) as f32 / 360.0))
}

fn hsv_to_rgb(hue: f32) -> [u8; 3] {
    let sector = hue.rem_euclid(1.0) * 6.0;
    let rising = (sector.fract() * 255.0).round() as u8;
    let falling = 255 - rising;

    match sector as u8 {
        0 => [255, rising, 0],
        1 => [falling, 255, 0],
        2 => [0, 255, rising],
        3 => [0, falling, 255],
        4 => [rising, 0, 255],
        _ => [255, 0, falling],
    }
}

#[cfg(test)]
mod tests {
    use super::random_vivid_palette;

    #[test]
    fn random_palette_has_four_distinct_colors() {
        for _ in 0..32 {
            let palette = random_vivid_palette();
            for first in 0..palette.len() {
                for second in (first + 1)..palette.len() {
                    assert_ne!(palette[first], palette[second]);
                }
            }
        }
    }
}
