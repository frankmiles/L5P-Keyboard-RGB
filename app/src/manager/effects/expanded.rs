use std::{sync::atomic::Ordering, thread, time::Duration};

use legion_input::EventKind;
use legion_rgb_driver::{BaseEffects, SPEED_RANGE};
use rand::{rngs::ThreadRng, Rng};

use crate::{
    enums::{AudioVisualStyle, Direction, Effects, TypingStyle},
    manager::{
        effects::{
            audio::{AudioFeatures, AudioLevelMonitor},
            zones::KEY_ZONES,
        },
        profile::Profile,
        Inner,
    },
};

const FRAME_TIME: Duration = Duration::from_millis(35);

pub fn play(manager: &mut Inner, profile: &Profile, effect: Effects, rng: &mut ThreadRng) {
    let speed = profile.speed.clamp(*SPEED_RANGE.start(), *SPEED_RANGE.end());
    let direction = if profile.direction == Direction::Left { 1.0 } else { -1.0 };
    let colors = profile.rgb_array();
    let input_events = manager.input.as_ref().map(|input| input.subscribe());

    manager.keyboard.set_effect(BaseEffects::Static).unwrap();

    let mut frame_index = 0_u64;
    let mut matrix_trail = [0.0_f32; 4];
    let mut fire_heat = [0.2_f32; 4];
    let mut star_brightness = [0.0_f32; 4];
    let mut shooting_star = (0_usize, 1_i32, 0_u8);
    let mut reactive_energy = [0.0_f32; 4];
    let mut typing_bloom = [0.0_f32; 4];
    let mut typing_wave = [0.0_f32; 4];
    let mut studio_energy = [0.0_f32; 4];
    let mut studio_heat = [0.0_f32; 4];
    let mut studio_spark = [0.0_f32; 4];
    let mut audio_history = [0.0_f32; 12];
    let mut audio_history_index = 0_usize;
    let mut smoothed_audio = [0.0_f32; 4];
    let mut previous_bass = 0.0_f32;
    let mut beat_energy = 0.0_f32;
    let mut audio_color_shift = 0.0_f32;
    let mut audio_monitor = match effect {
        Effects::AudioVisualizer { source, .. } => match AudioLevelMonitor::start(source, manager.stop_signals.manager_stop_signal.clone()) {
            Ok(monitor) => Some(monitor),
            Err(error) => {
                eprintln!("Audio visualizer unavailable: {error}");
                None
            }
        },
        _ => None,
    };

    while !manager.stop_signals.manager_stop_signal.load(Ordering::SeqCst) {
        let phase = frame_index as f32 * f32::from(speed) * 0.035;
        let mut frame = [0_u8; 12];

        let audio_error = audio_monitor.as_mut().and_then(AudioLevelMonitor::take_failure);
        if let Some(error) = audio_error {
            eprintln!("Audio visualizer stopped: {error}");
            audio_monitor = None;
        }

        match effect {
            Effects::Spectrum => {
                for zone in 0..4 {
                    put_zone(&mut frame, zone, hsv_to_rgb(phase * direction * 0.08 + zone as f32 * 0.25, 0.95, 1.0));
                }
            }
            Effects::MatrixRain => {
                for intensity in &mut matrix_trail {
                    *intensity *= 0.76;
                }
                let tick = (5 - u64::from(speed)).max(1);
                if frame_index % tick == 0 {
                    let head = ((frame_index / tick) as usize % 4 + if direction < 0.0 { 3 } else { 0 }) % 4;
                    matrix_trail[head] = 1.0;
                    if rng.random_range(0..100) < 28 {
                        matrix_trail[rng.random_range(0..4)] = 0.6;
                    }
                }
                for zone in 0..4 {
                    let intensity = matrix_trail[zone];
                    put_zone(&mut frame, zone, [0, (12.0 + intensity * 198.0) as u8, (8.0 + intensity * 145.0) as u8]);
                }
            }
            Effects::ReactiveTyping => {
                let mut typed_zones = [false; 4];
                if let Some(events) = input_events.as_ref() {
                    while let Ok(event) = events.try_recv() {
                        if let EventKind::KeyDown(key) = event.kind {
                            for (zone, keys) in KEY_ZONES.iter().enumerate() {
                                if keys.contains(&key) {
                                    reactive_energy[zone] = 1.0;
                                }
                            }
                        }
                    }
                }
                if let Some(input) = manager.input.as_ref() {
                    let held = input.pressed_keys();
                    for (zone, keys) in KEY_ZONES.iter().enumerate() {
                        typed_zones[zone] = keys.iter().any(|key| held.contains(key));
                    }
                }
                for zone in 0..4 {
                    if typed_zones[zone] {
                        reactive_energy[zone] = 1.0;
                    } else {
                        reactive_energy[zone] *= 0.89 - f32::from(speed - 1) * 0.035;
                    }
                    let idle = if manager.input.is_none() { 0.12 + 0.08 * (phase + zone as f32 * 0.7).sin().abs() } else { 0.0 };
                    let base_color = zone_color(&colors, zone);
                    let fallback = hsv_to_rgb(0.52 + zone as f32 * 0.1, 0.82, 1.0);
                    let color = if manager.input.is_none() { mix_rgb(base_color, fallback, 0.72) } else { base_color };
                    put_zone(&mut frame, zone, scale_rgb(color, reactive_energy[zone].max(idle)));
                }
            }
            Effects::TypingBloom => {
                let mut typed_zones = [false; 4];
                if let Some(events) = input_events.as_ref() {
                    while let Ok(event) = events.try_recv() {
                        if let EventKind::KeyDown(key) = event.kind {
                            for (zone, keys) in KEY_ZONES.iter().enumerate() {
                                if keys.contains(&key) {
                                    typing_bloom[zone] = 1.0;
                                }
                            }
                        }
                    }
                }
                if let Some(input) = manager.input.as_ref() {
                    let held = input.pressed_keys();
                    for (zone, keys) in KEY_ZONES.iter().enumerate() {
                        typed_zones[zone] = keys.iter().any(|key| held.contains(key));
                    }
                }
                for zone in 0..4 {
                    if typed_zones[zone] {
                        typing_bloom[zone] = 1.0;
                    } else {
                        typing_bloom[zone] *= 0.84 - f32::from(speed - 1) * 0.035;
                    }
                    let intensity = typing_bloom[zone];
                    let accent = hsv_to_rgb(0.52 + zone as f32 * 0.1, 0.82, 1.0);
                    let base_color = zone_color(&colors, zone);
                    let idle_color = if manager.input.is_none() { mix_rgb(base_color, accent, 0.72) } else { base_color };
                    let color = mix_rgb(idle_color, accent, intensity * 0.72);
                    let idle = if manager.input.is_none() { 0.1 + 0.08 * (phase + zone as f32).sin().abs() } else { 0.0 };
                    put_zone(&mut frame, zone, scale_rgb(color, (0.18 + intensity * 0.82).max(idle)));
                }
            }
            Effects::TypingWave => {
                let previous_wave = typing_wave;
                for zone in 0..4 {
                    let neighbor = match zone {
                        0 => previous_wave[1],
                        3 => previous_wave[2],
                        _ => previous_wave[zone - 1].max(previous_wave[zone + 1]),
                    };
                    typing_wave[zone] = (previous_wave[zone] * (0.83 - f32::from(speed - 1) * 0.035)).max(neighbor * 0.34);
                }
                if let Some(events) = input_events.as_ref() {
                    while let Ok(event) = events.try_recv() {
                        if let EventKind::KeyDown(key) = event.kind {
                            for (zone, keys) in KEY_ZONES.iter().enumerate() {
                                if keys.contains(&key) {
                                    typing_wave[zone] = 1.0;
                                }
                            }
                        }
                    }
                }
                if let Some(input) = manager.input.as_ref() {
                    let held = input.pressed_keys();
                    for (zone, keys) in KEY_ZONES.iter().enumerate() {
                        if keys.iter().any(|key| held.contains(key)) {
                            typing_wave[zone] = 1.0;
                        }
                    }
                }
                for zone in 0..4 {
                    let idle = if manager.input.is_none() { 0.1 + 0.08 * (phase + zone as f32 * 0.8).sin().abs() } else { 0.0 };
                    let base_color = zone_color(&colors, zone);
                    let fallback = hsv_to_rgb(0.52 + zone as f32 * 0.1, 0.82, 1.0);
                    let color = if manager.input.is_none() { mix_rgb(base_color, fallback, 0.72) } else { base_color };
                    put_zone(&mut frame, zone, scale_rgb(color, typing_wave[zone].max(idle)));
                }
            }
            Effects::TypingStudio { style } => {
                let mut typed_zones = [false; 4];
                if let Some(events) = input_events.as_ref() {
                    while let Ok(event) = events.try_recv() {
                        if let EventKind::KeyDown(key) = event.kind {
                            for (zone, keys) in KEY_ZONES.iter().enumerate() {
                                if keys.contains(&key) {
                                    typed_zones[zone] = true;
                                    studio_spark[zone] = 1.0;
                                    studio_heat[zone] = 1.0;
                                }
                            }
                        }
                    }
                }
                if let Some(input) = manager.input.as_ref() {
                    let held = input.pressed_keys();
                    for (zone, keys) in KEY_ZONES.iter().enumerate() {
                        typed_zones[zone] |= keys.iter().any(|key| held.contains(key));
                    }
                }

                let previous_energy = studio_energy;
                for zone in 0..4 {
                    studio_energy[zone] = if typed_zones[zone] { 1.0 } else { previous_energy[zone] * (0.82 - f32::from(speed - 1) * 0.035) };
                    studio_heat[zone] *= 0.94;
                    studio_spark[zone] *= 0.68;
                    let neighbor = match zone {
                        0 => previous_energy[1],
                        3 => previous_energy[2],
                        _ => previous_energy[zone - 1].max(previous_energy[zone + 1]),
                    };
                    typing_wave[zone] = (previous_energy[zone] * 0.68).max(neighbor * 0.52);
                }

                let moving_head = triangle_position(phase * direction * 0.45);
                let active_origin = studio_energy.iter().position(|energy| *energy > 0.85).unwrap_or(0);
                for zone in 0..4 {
                    let energy = studio_energy[zone];
                    let base = zone_color(&colors, zone);
                    let distance_to_origin = (zone as f32 - active_origin as f32).abs();
                    let distance_to_head = (zone as f32 - moving_head).abs();
                    let neighbor_glow = typing_wave[zone];
                    let (color, intensity) = match style {
                        TypingStyle::NeonRain => (mix_rgb(base, [0, 255, 190], energy), (energy + neighbor_glow * 0.58).clamp(0.0, 1.0)),
                        TypingStyle::Heartbeat => {
                            let beat = phase.rem_euclid(std::f32::consts::TAU);
                            let pulse = (-((beat / 0.11).powi(2))).exp() + 0.58 * (-(((beat - 0.19) / 0.09).powi(2))).exp();
                            (mix_rgb(base, [255, 35, 75], energy), energy * pulse.min(1.0))
                        }
                        TypingStyle::Prism => (
                            hsv_to_rgb(phase * 0.035 + zone as f32 * 0.25 + energy * 0.12, 0.92, 1.0),
                            (energy + neighbor_glow * 0.32).clamp(0.0, 1.0),
                        ),
                        TypingStyle::Meteor => {
                            let tail = (-distance_to_origin * 1.1).exp();
                            (mix_rgb(base, [180, 240, 255], energy), (energy * tail + neighbor_glow * 0.32).clamp(0.0, 1.0))
                        }
                        TypingStyle::Ember => {
                            let ember = mix_rgb([90, 4, 0], [255, 116, 10], studio_heat[zone]);
                            (mix_rgb(base, ember, studio_heat[zone]), (energy * 0.88 + studio_heat[zone] * 0.12).clamp(0.0, 1.0))
                        }
                        TypingStyle::Ripple => {
                            let ring = (1.0 - (distance_to_origin - phase.rem_euclid(4.0)).abs() / 1.1).clamp(0.0, 1.0);
                            (mix_rgb(base, [60, 190, 255], ring), (energy * 0.6 + ring * neighbor_glow).clamp(0.0, 1.0))
                        }
                        TypingStyle::Comet => {
                            let comet = (-distance_to_head * 1.5).exp();
                            (hsv_to_rgb(0.52 + zone as f32 * 0.04, 0.85, 1.0), (energy * 0.68 + comet * energy).clamp(0.0, 1.0))
                        }
                        TypingStyle::GhostTrail => (mix_rgb(base, [150, 100, 255], energy * 0.48), (energy * 0.72 + neighbor_glow * 0.48).clamp(0.0, 1.0)),
                        TypingStyle::ZoneBounce => {
                            let glow = (1.0 - distance_to_origin / 1.6).clamp(0.0, 1.0);
                            (mix_rgb(base, [255, 190, 40], energy), (energy * 0.7 + glow * neighbor_glow * 0.65).clamp(0.0, 1.0))
                        }
                        TypingStyle::Starlight => (
                            hsv_to_rgb(0.58 + zone as f32 * 0.035, 0.46, 1.0),
                            (studio_spark[zone] * (0.75 + rng.random_range(0.0..0.25))).clamp(0.0, 1.0),
                        ),
                        TypingStyle::Laser => {
                            let trail = if direction > 0.0 { zone <= active_origin } else { zone >= active_origin };
                            let intensity = if zone == active_origin {
                                energy
                            } else if trail {
                                energy * 0.3
                            } else {
                                0.0
                            };
                            ([255, 25, 55], intensity)
                        }
                        TypingStyle::RainbowSpark => (hsv_to_rgb(phase * 0.02 + zone as f32 * 0.25, 1.0, 1.0), studio_spark[zone]),
                        TypingStyle::Cascade => (hsv_to_rgb(0.34 + zone as f32 * 0.035, 0.92, 1.0), (energy + neighbor_glow * 0.72).clamp(0.0, 1.0)),
                        TypingStyle::Aurora => {
                            let drift = phase * 0.015 + zone as f32 * 0.13;
                            (hsv_to_rgb(0.42 + drift.sin() * 0.1, 0.68, 0.65 + energy * 0.35), (energy * 0.78 + neighbor_glow * 0.28).clamp(0.0, 1.0))
                        }
                        TypingStyle::Staccato => {
                            let tick = (5 - u64::from(speed)).max(1);
                            let gate = if (frame_index / tick) % 2 == 0 { 1.0 } else { 0.12 };
                            (mix_rgb(base, [255, 255, 255], energy * 0.8), energy * gate)
                        }
                    };
                    put_zone(&mut frame, zone, scale_rgb(color, intensity));
                }
            }
            Effects::AudioVisualizer { style, .. } => {
                let measured = audio_monitor.as_ref().map_or(AudioFeatures::default(), AudioLevelMonitor::features).to_array();
                let current = measured.map(normalize_audio_band);
                for band in 0..4 {
                    let attack = if current[band] > smoothed_audio[band] { 0.72 } else { 0.28 };
                    smoothed_audio[band] += (current[band] - smoothed_audio[band]) * attack;
                }
                let [bass, mid, treble, voice] = smoothed_audio;
                let broad_level = bass.max(mid).max(treble).max(voice);
                let bass_onset = (bass - previous_bass - 0.04).max(0.0);
                if bass_onset > 0.08 {
                    beat_energy = (bass_onset * 3.2).clamp(0.0, 1.0);
                    audio_color_shift = (audio_color_shift + 0.13).rem_euclid(1.0);
                } else {
                    beat_energy *= 0.74;
                }
                previous_bass = bass;

                audio_history[audio_history_index] = broad_level;
                audio_history_index = (audio_history_index + 1) % audio_history.len();
                let spectrum = [bass, bass * 0.6 + mid * 0.4, mid * 0.65 + treble * 0.35, voice.max(treble * 0.6)];

                for zone in 0..4 {
                    let intensity = match style {
                        AudioVisualStyle::VuMeter => broad_level * (1.0 - zone as f32 * 0.12),
                        AudioVisualStyle::BeatPulse => beat_energy * (0.85 + (zone % 2) as f32 * 0.15),
                        AudioVisualStyle::Waveform => {
                            let offset = zone * 3;
                            let index = (audio_history_index + audio_history.len() - offset % audio_history.len()) % audio_history.len();
                            audio_history[index]
                        }
                        AudioVisualStyle::BassDrop => (bass * 0.45 + beat_energy * if zone < 2 { 0.9 } else { 0.35 }).clamp(0.0, 1.0),
                        AudioVisualStyle::SpectrumBars => spectrum[zone],
                        AudioVisualStyle::VocalGlow => voice * (0.86 + (zone as f32 - 1.5).abs() * 0.08),
                        AudioVisualStyle::TrebleSpark => (treble * (0.5 + rng.random_range(0.0..0.5))).clamp(0.0, 1.0),
                        AudioVisualStyle::BeatStrobe => {
                            if beat_energy > 0.42 {
                                1.0
                            } else {
                                0.0
                            }
                        }
                        AudioVisualStyle::Equalizer => spectrum[(zone + frame_index as usize / 5) % 4],
                        AudioVisualStyle::SoundRipple => {
                            let radius = (frame_index as f32 * 0.14 * direction).rem_euclid(4.0);
                            let distance = circular_distance(zone as f32, radius, 4.0);
                            beat_energy * (-distance * 1.8).exp()
                        }
                        AudioVisualStyle::DanceFloor => {
                            let pattern = (frame_index as f32 * 0.18 + zone as f32 * 1.57).sin().abs();
                            broad_level * (0.35 + pattern * 0.65)
                        }
                        AudioVisualStyle::Aurora => {
                            let drift = frame_index as f32 * 0.025 + zone as f32 * 0.45;
                            broad_level * (0.45 + drift.sin().abs() * 0.55)
                        }
                        AudioVisualStyle::Siren => {
                            let sweep = (frame_index as f32 * 0.12 * direction).rem_euclid(4.0);
                            broad_level * (1.0 - (zone as f32 - sweep).abs() / 2.0).clamp(0.0, 1.0)
                        }
                        AudioVisualStyle::FireBeat => (bass * (0.45 + 0.55 * rng.random_range(0.0..1.0)) + beat_energy * 0.8).clamp(0.0, 1.0),
                        AudioVisualStyle::CometTrail => {
                            let offset = (audio_history_index + audio_history.len() - zone * 2 % audio_history.len()) % audio_history.len();
                            audio_history[offset] * (1.0 - zone as f32 * 0.12)
                        }
                    };
                    let color = match style {
                        AudioVisualStyle::VuMeter | AudioVisualStyle::SpectrumBars | AudioVisualStyle::Equalizer => hsv_to_rgb(0.34 - zone as f32 * 0.105, 0.96, 1.0),
                        AudioVisualStyle::BeatPulse | AudioVisualStyle::BassDrop | AudioVisualStyle::BeatStrobe | AudioVisualStyle::FireBeat => {
                            mix_rgb(zone_color(&colors, zone), [255, 48, 16], beat_energy)
                        }
                        AudioVisualStyle::Waveform | AudioVisualStyle::CometTrail => hsv_to_rgb(0.52 + zone as f32 * 0.075, 0.88, 1.0),
                        AudioVisualStyle::VocalGlow => hsv_to_rgb(0.78 + zone as f32 * 0.035, 0.62, 1.0),
                        AudioVisualStyle::TrebleSpark => hsv_to_rgb(0.53 + zone as f32 * 0.09, 0.86, 1.0),
                        AudioVisualStyle::SoundRipple => mix_rgb(zone_color(&colors, zone), [25, 170, 255], beat_energy),
                        AudioVisualStyle::DanceFloor => hsv_to_rgb(audio_color_shift + zone as f32 * 0.25, 0.95, 1.0),
                        AudioVisualStyle::Aurora => hsv_to_rgb(0.38 + zone as f32 * 0.045 + audio_color_shift * 0.1, 0.7, 1.0),
                        AudioVisualStyle::Siren => {
                            if zone % 2 == 0 {
                                [255, 15, 20]
                            } else {
                                [20, 45, 255]
                            }
                        }
                    };
                    put_zone(&mut frame, zone, scale_rgb(color, intensity));
                }
            }
            Effects::Pulse => {
                let beat = (phase * 3.5).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
                let first = (-((beat / 0.095).powi(2))).exp();
                let second = (-(((beat - 0.19) / 0.085).powi(2))).exp() * 0.62;
                let intensity = (0.12 + (first + second).min(1.0) * 0.88).clamp(0.0, 1.0);
                for zone in 0..4 {
                    put_zone(&mut frame, zone, scale_rgb(zone_color(&colors, zone), intensity));
                }
            }
            Effects::Aurora => {
                for zone in 0..4 {
                    let drift = phase * 0.045 + zone as f32 * 0.13;
                    let hue = 0.38 + drift.sin() * 0.11 + (drift * 0.57).cos() * 0.08;
                    let value = 0.42 + 0.48 * ((drift + zone as f32 * 0.3).sin() * 0.5 + 0.5);
                    put_zone(&mut frame, zone, hsv_to_rgb(hue, 0.72, value));
                }
            }
            Effects::SnakeTrail => {
                let position = triangle_position(phase * direction);
                for zone in 0..4 {
                    let distance = (zone as f32 - position).abs();
                    let glow = (-distance * 1.45).exp();
                    put_zone(&mut frame, zone, scale_rgb(zone_color(&colors, zone), glow));
                }
            }
            Effects::RadarSweep => {
                let position = (phase * 0.85 * direction).rem_euclid(4.0);
                for zone in 0..4 {
                    let distance = circular_distance(zone as f32, position, 4.0);
                    let beam = (-distance * 2.4).exp();
                    let color = mix_rgb([0, 5, 24], zone_color(&colors, zone), beam);
                    put_zone(&mut frame, zone, color);
                }
            }
            Effects::Fire => {
                for zone in 0..4 {
                    if frame_index % (5 - u64::from(speed)).max(1) == 0 {
                        fire_heat[zone] = (fire_heat[zone] * 0.58 + rng.random_range(0.0..1.0) * 0.42).clamp(0.05, 1.0);
                    }
                    let heat = fire_heat[zone];
                    let color = if heat < 0.5 {
                        mix_rgb([105, 5, 0], [255, 65, 0], heat * 2.0)
                    } else {
                        mix_rgb([255, 65, 0], [255, 226, 70], (heat - 0.5) * 2.0)
                    };
                    let spark = if rng.random_range(0..100) < 4 { 1.0 } else { heat };
                    put_zone(&mut frame, zone, scale_rgb(color, spark));
                }
            }
            Effects::Starfield => {
                for brightness in &mut star_brightness {
                    *brightness *= 0.76 - f32::from(speed - 1) * 0.04;
                    if rng.random_range(0..100) < 5 + u32::from(speed) * 2 {
                        *brightness = 1.0;
                    }
                }
                if frame_index % (60 / u64::from(speed)).max(1) == 0 {
                    shooting_star = (rng.random_range(0..4), if rng.random_range(0..2) == 0 { -1 } else { 1 }, 0);
                }
                let tick = (5 - u64::from(speed)).max(1);
                if frame_index % tick == 0 && shooting_star.2 < 12 {
                    let position = shooting_star.0 as i32 + shooting_star.1 * (shooting_star.2 as i32 / tick as i32);
                    if (0..4).contains(&position) {
                        star_brightness[position as usize] = 1.0;
                    }
                    shooting_star.2 += 1;
                }
                for zone in 0..4 {
                    let star = hsv_to_rgb(0.58 + zone as f32 * 0.025, 0.55, star_brightness[zone]);
                    put_zone(&mut frame, zone, star);
                }
            }
            Effects::Marquee => {
                let position = (phase * 0.7 * direction).rem_euclid(4.0);
                for zone in 0..4 {
                    let distance = circular_distance(zone as f32, position, 4.0);
                    let band = (1.0 - distance / 1.25).clamp(0.0, 1.0);
                    let color = hsv_to_rgb(phase * 0.025 + zone as f32 * 0.08, 0.9, 0.18 + 0.82 * band);
                    put_zone(&mut frame, zone, color);
                }
            }
            Effects::BouncingBall => {
                let position = triangle_position(phase * direction);
                for zone in 0..4 {
                    let distance = (zone as f32 - position).abs();
                    let glow = (1.0 - distance / 1.4).clamp(0.0, 1.0).powi(2);
                    put_zone(&mut frame, zone, scale_rgb(zone_color(&colors, zone), glow));
                }
            }
            Effects::LaserBeamSweep => {
                let position = (phase * 1.1 * direction).rem_euclid(4.0);
                for zone in 0..4 {
                    let distance = circular_distance(zone as f32, position, 4.0);
                    let beam = (-distance * 7.0).exp();
                    put_zone(&mut frame, zone, scale_rgb(zone_color(&colors, zone), 0.025 + 0.975 * beam));
                }
            }
            Effects::DualColorSplit => {
                let blend = (phase * 0.24).sin() * 0.5 + 0.5;
                let left = hsv_to_rgb(0.94 + blend * 0.08, 0.9, 1.0);
                let right = hsv_to_rgb(0.49 + blend * 0.08, 0.95, 1.0);
                for zone in 0..4 {
                    let color = match zone {
                        0 => left,
                        1 => mix_rgb(left, right, 0.12),
                        2 => mix_rgb(right, left, 0.12),
                        _ => right,
                    };
                    put_zone(&mut frame, zone, color);
                }
            }
            Effects::NeonSineWave => {
                for zone in 0..4 {
                    let wave = ((phase * 0.7 * direction + zone as f32 * 1.25).sin() + 1.0) * 0.5;
                    let hue = if zone % 2 == 0 { 0.84 + wave * 0.06 } else { 0.48 + wave * 0.08 };
                    put_zone(&mut frame, zone, hsv_to_rgb(hue, 0.92, 0.18 + wave * 0.82));
                }
            }
            Effects::RippleBurst => {
                let cycle_frame = frame_index * u64::from(speed);
                let burst = cycle_frame / 100;
                let origin = if direction > 0.0 { burst as usize % 4 } else { 3 - burst as usize % 4 };
                let age = (cycle_frame % 100) as f32 / 100.0;
                let radius = age * 4.0;
                let envelope = (1.0 - age).powf(0.65);
                for zone in 0..4 {
                    let distance = circular_distance(zone as f32, origin as f32, 4.0);
                    let wave = (1.0 - ((distance - radius).abs() / 0.8)).clamp(0.0, 1.0);
                    put_zone(&mut frame, zone, scale_rgb(zone_color(&colors, zone), wave * envelope));
                }
            }
            Effects::StrobeFlash => {
                let half_period = (5 - u64::from(speed)).max(1);
                for zone in 0..4 {
                    let alternating = (frame_index / half_period + zone as u64) % 2 == 0;
                    let intensity = if alternating { 1.0 } else { 0.015 };
                    put_zone(&mut frame, zone, scale_rgb(zone_color(&colors, zone), intensity));
                }
            }
            _ => return,
        }

        manager.keyboard.set_colors_to(&frame).unwrap();
        frame_index = frame_index.wrapping_add(1);
        thread::sleep(FRAME_TIME);
    }
}

fn put_zone(frame: &mut [u8; 12], zone: usize, color: [u8; 3]) {
    let start = zone * 3;
    frame[start..start + 3].copy_from_slice(&color);
}

fn zone_color(colors: &[u8; 12], zone: usize) -> [u8; 3] {
    let start = zone * 3;
    [colors[start], colors[start + 1], colors[start + 2]]
}

fn scale_rgb(color: [u8; 3], intensity: f32) -> [u8; 3] {
    color.map(|channel| (f32::from(channel) * intensity.clamp(0.0, 1.0)).round() as u8)
}

fn normalize_audio_band(level: f32) -> f32 {
    ((level - 0.008).max(0.0) * 5.0).clamp(0.0, 1.0)
}

fn mix_rgb(start: [u8; 3], end: [u8; 3], amount: f32) -> [u8; 3] {
    std::array::from_fn(|index| (f32::from(start[index]) + (f32::from(end[index]) - f32::from(start[index])) * amount.clamp(0.0, 1.0)).round() as u8)
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> [u8; 3] {
    let hue = hue.rem_euclid(1.0) * 6.0;
    let chroma = value * saturation;
    let x = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match hue as u8 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let match_value = value - chroma;
    [
        ((red + match_value) * 255.0).round() as u8,
        ((green + match_value) * 255.0).round() as u8,
        ((blue + match_value) * 255.0).round() as u8,
    ]
}

fn triangle_position(phase: f32) -> f32 {
    let position = phase.rem_euclid(6.0);
    if position <= 3.0 {
        position
    } else {
        6.0 - position
    }
}

fn circular_distance(first: f32, second: f32, circumference: f32) -> f32 {
    let distance = (first - second).abs().rem_euclid(circumference);
    distance.min(circumference - distance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hue_conversion_produces_primary_colors() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(hsv_to_rgb(1.0 / 3.0, 1.0, 1.0), [0, 255, 0]);
        assert_eq!(hsv_to_rgb(2.0 / 3.0, 1.0, 1.0), [0, 0, 255]);
    }

    #[test]
    fn positions_and_color_scaling_stay_inside_keyboard_bounds() {
        assert_eq!(triangle_position(0.0), 0.0);
        assert_eq!(triangle_position(3.0), 3.0);
        assert_eq!(triangle_position(4.0), 2.0);
        assert_eq!(circular_distance(0.0, 3.0, 4.0), 1.0);
        assert_eq!(scale_rgb([100, 200, 255], 0.5), [50, 100, 128]);
    }
}
