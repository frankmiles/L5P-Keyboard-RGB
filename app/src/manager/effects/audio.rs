use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, RecvTimeoutError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::enums::AudioSource;

const WINDOW_SIZE: usize = 1024;
const FEATURE_COUNT: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioFeatures {
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    pub voice: f32,
}

impl AudioFeatures {
    fn from_samples(samples: &[f32]) -> Self {
        Self {
            bass: band_level(samples, &[70.0, 110.0, 170.0, 230.0]),
            mid: band_level(samples, &[300.0, 550.0, 900.0, 1400.0, 2100.0]),
            treble: band_level(samples, &[2800.0, 4200.0, 6200.0, 8200.0]),
            voice: band_level(samples, &[180.0, 260.0, 380.0, 560.0, 820.0, 1200.0, 1800.0, 2600.0, 3400.0]),
        }
    }

    pub(super) fn to_array(self) -> [f32; FEATURE_COUNT] {
        [self.bass, self.mid, self.treble, self.voice]
    }
}

pub struct AudioLevelMonitor {
    features: Arc<[AtomicU32; FEATURE_COUNT]>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), String>>>,
}

impl AudioLevelMonitor {
    pub fn start(source: AudioSource, manager_stop: Arc<AtomicBool>) -> Result<Self, String> {
        let features = Arc::new(std::array::from_fn(|_| AtomicU32::new(0.0_f32.to_bits())));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_features = Arc::clone(&features);
        let worker_stop = Arc::clone(&stop);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("legion-audio-analyzer".to_string())
            .spawn(move || monitor_worker(source, manager_stop, worker_stop, worker_features, ready_tx))
            .map_err(|error| format!("Could not start audio analyzer thread: {error}"))?;

        match ready_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(())) => Ok(Self { features, stop, worker: Some(worker) }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(RecvTimeoutError::Disconnected) => {
                let result = worker.join();
                Err(match result {
                    Ok(Err(error)) => error,
                    Ok(Ok(())) => "Audio analyzer stopped before initialization completed.".to_string(),
                    Err(_) => "Audio analyzer thread stopped unexpectedly.".to_string(),
                })
            }
            Err(RecvTimeoutError::Timeout) => {
                stop.store(true, Ordering::SeqCst);
                let _ = worker.join();
                Err("Audio analyzer initialization timed out.".to_string())
            }
        }
    }

    pub fn features(&self) -> AudioFeatures {
        let values = self.features.each_ref().map(|value| f32::from_bits(value.load(Ordering::Relaxed)).clamp(0.0, 1.0));
        AudioFeatures {
            bass: values[0],
            mid: values[1],
            treble: values[2],
            voice: values[3],
        }
    }

    pub fn take_failure(&mut self) -> Option<String> {
        let worker = self.worker.as_ref()?;
        if !worker.is_finished() {
            return None;
        }

        match self.worker.take().unwrap().join() {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error),
            Err(_) => Some("Audio analyzer thread stopped unexpectedly.".to_string()),
        }
    }
}

impl Drop for AudioLevelMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            match worker.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("Audio visualizer stopped: {error}"),
                Err(_) => eprintln!("Audio visualizer thread stopped unexpectedly."),
            }
        }
    }
}

fn band_level(samples: &[f32], frequencies: &[f32]) -> f32 {
    if samples.len() != WINDOW_SIZE {
        return 0.0;
    }

    frequencies.iter().map(|frequency| goertzel_amplitude(samples, *frequency, 44100.0)).fold(0.0, f32::max)
}

fn goertzel_amplitude(samples: &[f32], frequency: f32, sample_rate: f32) -> f32 {
    let omega = std::f32::consts::TAU * frequency / sample_rate;
    let coefficient = 2.0 * omega.cos();
    let mut previous = 0.0;
    let mut previous_previous = 0.0;
    let mut window_sum = 0.0;

    for (index, sample) in samples.iter().enumerate() {
        let window = 0.5 - 0.5 * (std::f32::consts::TAU * index as f32 / (samples.len() - 1) as f32).cos();
        let current = sample * window + coefficient * previous - previous_previous;
        previous_previous = previous;
        previous = current;
        window_sum += window;
    }

    let power = (previous * previous + previous_previous * previous_previous - coefficient * previous * previous_previous).max(0.0);
    (2.0 * power.sqrt() / window_sum).clamp(0.0, 1.0)
}

#[cfg(target_os = "windows")]
fn monitor_worker(
    source: AudioSource, manager_stop: Arc<AtomicBool>, local_stop: Arc<AtomicBool>, features: Arc<[AtomicU32; FEATURE_COUNT]>, ready: mpsc::SyncSender<Result<(), String>>,
) -> Result<(), String> {
    use std::collections::VecDeque;

    use wasapi::{DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

    wasapi::initialize_mta().ok().map_err(|error| format!("Could not initialize Windows audio: {error}"))?;
    let _apartment = WasapiApartment;
    let enumerator = DeviceEnumerator::new().map_err(|error| format!("Could not enumerate audio devices: {error}"))?;
    let endpoint_direction = match source {
        AudioSource::System => Direction::Render,
        AudioSource::Microphone => Direction::Capture,
    };
    let device = enumerator
        .get_default_device(&endpoint_direction)
        .map_err(|error| format!("Could not open the default {source} device: {error}"))?;
    let mut audio_client = device.get_iaudioclient().map_err(|error| format!("Could not open the {source} audio stream: {error}"))?;
    let format = WaveFormat::new(32, 32, &SampleType::Float, 44100, 2, None);
    let stream_mode = StreamMode::PollingShared {
        autoconvert: true,
        buffer_duration_hns: 1_000_000,
    };
    let stream_direction = Direction::Capture;
    audio_client
        .initialize_client(&format, &stream_direction, &stream_mode)
        .map_err(|error| format!("Could not initialize the {source} audio stream: {error}"))?;
    let capture = audio_client.get_audiocaptureclient().map_err(|error| format!("Could not read the {source} audio stream: {error}"))?;
    audio_client.start_stream().map_err(|error| format!("Could not start the {source} audio stream: {error}"))?;
    ready.send(Ok(())).map_err(|_| "Audio visualizer stopped during initialization.".to_string())?;

    let mut queued_bytes = VecDeque::with_capacity(WINDOW_SIZE * format.get_blockalign() as usize);
    let mut mono_samples = Vec::with_capacity(WINDOW_SIZE);
    let mut audio_bytes = vec![0_u8; WINDOW_SIZE * format.get_blockalign() as usize];
    let mut last_analysis = Instant::now();

    while !manager_stop.load(Ordering::SeqCst) && !local_stop.load(Ordering::SeqCst) {
        if capture.get_next_packet_size().map_err(|error| format!("Could not inspect {source} audio data: {error}"))?.unwrap_or(0) > 0 {
            capture
                .read_from_device_to_deque(&mut queued_bytes)
                .map_err(|error| format!("Could not read {source} audio data: {error}"))?;

            while queued_bytes.len() >= audio_bytes.len() {
                for byte in &mut audio_bytes {
                    *byte = queued_bytes.pop_front().unwrap();
                }
                mono_samples.clear();
                for frame in audio_bytes.chunks_exact(8) {
                    let left = f32::from_ne_bytes(frame[0..4].try_into().unwrap());
                    let right = f32::from_ne_bytes(frame[4..8].try_into().unwrap());
                    mono_samples.push(((left + right) * 0.5).clamp(-1.0, 1.0));
                }
                let analyzed = AudioFeatures::from_samples(&mono_samples);
                for (target, value) in features.iter().zip(analyzed.to_array()) {
                    target.store(value.to_bits(), Ordering::Relaxed);
                }
                last_analysis = Instant::now();
            }
        } else if last_analysis.elapsed() > Duration::from_millis(120) {
            for value in features.iter() {
                value.store(0.0_f32.to_bits(), Ordering::Relaxed);
            }
            last_analysis = Instant::now();
        }
        thread::sleep(Duration::from_millis(5));
    }

    audio_client.stop_stream().map_err(|error| format!("Could not stop the {source} audio stream: {error}"))?;
    Ok(())
}

#[cfg(target_os = "windows")]
struct WasapiApartment;

#[cfg(target_os = "windows")]
impl Drop for WasapiApartment {
    fn drop(&mut self) {
        wasapi::deinitialize();
    }
}

#[cfg(not(target_os = "windows"))]
fn monitor_worker(
    _source: AudioSource, _manager_stop: Arc<AtomicBool>, _local_stop: Arc<AtomicBool>, _features: Arc<[AtomicU32; FEATURE_COUNT]>, ready: mpsc::SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let error = "Audio visualization input is currently supported on Windows only.".to_string();
    let _ = ready.send(Err(error.clone()));
    Err(error)
}

#[cfg(test)]
mod tests {
    use super::{band_level, AudioFeatures, WINDOW_SIZE};

    #[test]
    fn audio_features_detect_the_matching_frequency_band() {
        let samples: Vec<f32> = (0..WINDOW_SIZE).map(|index| (std::f32::consts::TAU * 110.0 * index as f32 / 44100.0).sin() * 0.5).collect();
        let features = AudioFeatures::from_samples(&samples);

        assert!(features.bass > features.mid);
        assert!(features.bass > features.treble);
    }

    #[test]
    fn silence_produces_no_visualizer_energy() {
        let silence = [0.0; WINDOW_SIZE];
        assert_eq!(band_level(&silence, &[110.0, 440.0]), 0.0);
        assert_eq!(AudioFeatures::from_samples(&silence), AudioFeatures::default());
    }
}
