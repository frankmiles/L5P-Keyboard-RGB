use crate::manager::{custom_effect::CustomEffect, profile::Profile};
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter, EnumString, IntoStaticStr};

#[derive(Clone, Copy, EnumString, Serialize, Deserialize, Display, EnumIter, Debug, IntoStaticStr, Default)]
pub enum Effects {
    #[default]
    Static,
    Breath,
    Smooth,
    Wave,
    Lightning,
    AmbientLight {
        fps: u8,
        saturation_boost: f32,
    },
    SmoothWave {
        mode: SwipeMode,
        clean_with_black: bool,
    },
    Swipe {
        mode: SwipeMode,
        clean_with_black: bool,
    },
    Disco,
    Christmas,
    Fade,
    Temperature,
    Ripple,
    Spectrum,
    MatrixRain,
    ReactiveTyping,
    Pulse,
    Aurora,
    SnakeTrail,
    RadarSweep,
    Fire,
    Starfield,
    Marquee,
    BouncingBall,
    LaserBeamSweep,
    DualColorSplit,
    NeonSineWave,
    RippleBurst,
    StrobeFlash,
    TypingBloom,
    TypingWave,
    TypingStudio {
        style: TypingStyle,
    },
    AudioVisualizer {
        source: AudioSource,
        style: AudioVisualStyle,
    },
}

#[derive(Clone, Copy, EnumString, Serialize, Deserialize, Display, EnumIter, Debug, Default, PartialEq, Eq)]
pub enum AudioSource {
    #[default]
    System,
    Microphone,
}

#[derive(Clone, Copy, EnumString, Serialize, Deserialize, Display, EnumIter, Debug, Default, PartialEq, Eq)]
pub enum AudioVisualStyle {
    #[default]
    VuMeter,
    BeatPulse,
    Waveform,
    BassDrop,
    SpectrumBars,
    VocalGlow,
    TrebleSpark,
    BeatStrobe,
    Equalizer,
    SoundRipple,
    DanceFloor,
    Aurora,
    Siren,
    FireBeat,
    CometTrail,
}

#[derive(Clone, Copy, EnumString, Serialize, Deserialize, Display, EnumIter, Debug, Default, PartialEq, Eq)]
pub enum TypingStyle {
    #[default]
    NeonRain,
    Heartbeat,
    Prism,
    Meteor,
    Ember,
    Ripple,
    Comet,
    GhostTrail,
    ZoneBounce,
    Starlight,
    Laser,
    RainbowSpark,
    Cascade,
    Aurora,
    Staccato,
}

#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize, EnumIter, EnumString, PartialEq)]
pub enum SwipeMode {
    #[default]
    Change,
    Fill,
}

impl PartialEq for Effects {
    fn eq(&self, other: &Self) -> bool {
        core::mem::discriminant(self) == core::mem::discriminant(other)
    }
}

#[allow(dead_code)]
impl Effects {
    pub fn takes_color_array(self) -> bool {
        matches!(
            self,
            Self::Static
                | Self::Breath
                | Self::Lightning
                | Self::Swipe { .. }
                | Self::Fade
                | Self::Ripple
                | Self::ReactiveTyping
                | Self::Pulse
                | Self::SnakeTrail
                | Self::RadarSweep
                | Self::Marquee
                | Self::BouncingBall
                | Self::LaserBeamSweep
                | Self::RippleBurst
                | Self::StrobeFlash
                | Self::TypingBloom
                | Self::TypingWave
                | Self::TypingStudio { .. }
                | Self::AudioVisualizer { .. }
        )
    }

    pub fn takes_direction(self) -> bool {
        matches!(
            self,
            Self::Wave
                | Self::SmoothWave { .. }
                | Self::Swipe { .. }
                | Self::Spectrum
                | Self::MatrixRain
                | Self::SnakeTrail
                | Self::RadarSweep
                | Self::Marquee
                | Self::BouncingBall
                | Self::LaserBeamSweep
                | Self::NeonSineWave
                | Self::RippleBurst
                | Self::TypingStudio { .. }
        )
    }

    pub fn takes_speed(self) -> bool {
        matches!(
            self,
            Self::Breath
                | Self::Smooth
                | Self::Wave
                | Self::Lightning
                | Self::SmoothWave { .. }
                | Self::Swipe { .. }
                | Self::Disco
                | Self::Fade
                | Self::Ripple
                | Self::Spectrum
                | Self::MatrixRain
                | Self::ReactiveTyping
                | Self::Pulse
                | Self::Aurora
                | Self::SnakeTrail
                | Self::RadarSweep
                | Self::Fire
                | Self::Starfield
                | Self::Marquee
                | Self::BouncingBall
                | Self::LaserBeamSweep
                | Self::DualColorSplit
                | Self::NeonSineWave
                | Self::RippleBurst
                | Self::StrobeFlash
                | Self::TypingBloom
                | Self::TypingWave
                | Self::TypingStudio { .. }
                | Self::AudioVisualizer { .. }
        )
    }

    pub fn is_built_in(self) -> bool {
        matches!(
            self,
            Self::Static
                | Self::Breath
                | Self::Smooth
                | Self::Wave
                | Self::Spectrum
                | Self::MatrixRain
                | Self::ReactiveTyping
                | Self::Pulse
                | Self::Aurora
                | Self::SnakeTrail
                | Self::RadarSweep
                | Self::Fire
                | Self::Starfield
                | Self::Marquee
                | Self::BouncingBall
                | Self::LaserBeamSweep
                | Self::DualColorSplit
                | Self::NeonSineWave
                | Self::RippleBurst
                | Self::StrobeFlash
                | Self::TypingBloom
                | Self::TypingWave
                | Self::TypingStudio { .. }
                | Self::AudioVisualizer { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioVisualStyle, Effects, TypingStyle};
    use strum::IntoEnumIterator;

    #[test]
    fn expanded_effects_are_exposed_as_builtin_speed_controls() {
        let effects = [
            Effects::Spectrum,
            Effects::MatrixRain,
            Effects::ReactiveTyping,
            Effects::Pulse,
            Effects::Aurora,
            Effects::SnakeTrail,
            Effects::RadarSweep,
            Effects::Fire,
            Effects::Starfield,
            Effects::Marquee,
            Effects::BouncingBall,
            Effects::LaserBeamSweep,
            Effects::DualColorSplit,
            Effects::NeonSineWave,
            Effects::RippleBurst,
            Effects::StrobeFlash,
            Effects::TypingBloom,
            Effects::TypingWave,
            Effects::TypingStudio { style: TypingStyle::NeonRain },
            Effects::AudioVisualizer {
                source: super::AudioSource::System,
                style: super::AudioVisualStyle::VuMeter,
            },
        ];

        assert!(effects.into_iter().all(Effects::is_built_in));
        assert!(effects.into_iter().all(Effects::takes_speed));
        assert!(Effects::MatrixRain.takes_direction());
        assert!(Effects::ReactiveTyping.takes_color_array());
        assert!(!Effects::Spectrum.takes_color_array());
        assert!(Effects::AudioVisualizer {
            source: super::AudioSource::System,
            style: super::AudioVisualStyle::VuMeter,
        }
        .takes_color_array());
        assert_eq!(AudioVisualStyle::iter().count(), 15);
        assert_eq!(TypingStyle::iter().count(), 15);
    }
}

#[derive(Clone, Copy, EnumString, Serialize, Deserialize, Debug, EnumIter, IntoStaticStr, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Left,
    Right,
}

#[derive(PartialEq, Eq, EnumIter, IntoStaticStr, Clone, Copy, Default, Serialize, Deserialize, Debug, Display, EnumString)]
pub enum Brightness {
    #[default]
    Low,
    High,
}

#[derive(Debug)]
pub enum Message {
    CustomEffect { effect: CustomEffect },
    Profile { profile: Profile },
    Exit,
}
