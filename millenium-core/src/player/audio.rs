// This file is part of Millenium Player.
// Copyright (C) 2023 John DiSanti.
//
// Millenium Player is free software: you can redistribute it and/or modify it under the terms of
// the GNU General Public License as published by the Free Software Foundation, either version 3 of
// the License, or (at your option) any later version.
//
// Millenium Player is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See
// the GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along with Millenium Player.
// If not, see <https://www.gnu.org/licenses/>.

use super::sink::{AudioBuffer, BoxAudioBuffer, Sink};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BuildStreamError, Device, DeviceNameError, Host, OutputCallbackInfo, PauseStreamError,
    PlayStreamError, Sample, SampleFormat, SampleRate, Stream, StreamError, SupportedStreamConfig,
    SupportedStreamConfigRange, SupportedStreamConfigsError,
};
use std::{
    cmp::Ordering,
    sync::{
        atomic::{self, AtomicBool, AtomicU64},
        mpsc::Receiver,
        Arc, Mutex, OnceLock,
    },
};

const PREFERRED_SAMPLE_RATES: &[u32] = &[48000, 44100, 88200, 96000];

#[derive(Debug, thiserror::Error)]
pub enum AudioDeviceError {
    #[error("failed to query audio devices: {0}")]
    FailedToQueryDevices(#[source] cpal::DevicesError),
    #[error("failed to get audio device name: {0}")]
    FailedToGetDeviceName(
        #[from]
        #[source]
        DeviceNameError,
    ),
    #[error("no default audio output device")]
    NoDefaultAudioOutputDevice,
    #[error("failed to query supported stream configs from output audio device: {0}")]
    FailedToQuerySupportedStreamConfigs(
        #[from]
        #[source]
        SupportedStreamConfigsError,
    ),
    #[error("failed to find supported stream config on the audio output device")]
    FailedToSelectConfig,
    #[error("failed to create the audio output stream: {0}")]
    FailedToCreateStream(
        #[from]
        #[source]
        BuildStreamError,
    ),
    #[error("the audio stream failed: {0}")]
    StreamFailed(
        #[from]
        #[source]
        StreamError,
    ),
    #[error("failed to play stream: {0}")]
    FailedToPlayStream(
        #[from]
        #[source]
        PlayStreamError,
    ),
    #[error("failed to pause stream: {0}")]
    FailedToPauseStream(
        #[from]
        #[source]
        PauseStreamError,
    ),
}

/// Represents an output device that can play audio.
pub(super) trait AudioDevice {
    /// Create a sink for the given sample rate and number of channels.
    fn create_sink(&self, input_sample_rate: u32, input_channels: usize) -> Sink;

    /// Returns the sample rate that playback occurs at.
    fn playback_sample_rate(&self) -> usize;

    /// Returns the amount of audio data consumed in number of frames.
    fn frames_consumed(&self) -> u64;

    /// Resets the amount of consumed audio data.
    fn reset_frames_consumed(&self);

    /// Stops playback and clears the queue.
    fn stop(&self) -> Result<(), AudioDeviceError>;

    /// Starts playback on the device.
    fn play(&self) -> Result<(), AudioDeviceError>;

    /// Pauses playback on the device.
    fn pause(&self) -> Result<(), AudioDeviceError>;

    /// Checks the device for errors.
    fn healthcheck(&self) -> Result<(), AudioDeviceError>;
}

pub(super) struct NullAudioDevice {
    config: SupportedStreamConfig,
    output_buffer: Arc<Mutex<BoxAudioBuffer>>,
    frames_consumed: AtomicU64,
}

impl NullAudioDevice {
    pub(super) fn new() -> Self {
        Self {
            config: SupportedStreamConfig::new(
                2,
                SampleRate(44100),
                cpal::SupportedBufferSize::Unknown,
                SampleFormat::F32,
            ),
            output_buffer: Arc::new(Mutex::new(BoxAudioBuffer::new(
                SampleFormat::F32,
                AudioBuffer::new(Vec::<f32>::new()),
            ))),
            frames_consumed: AtomicU64::new(0),
        }
    }
}

impl AudioDevice for NullAudioDevice {
    fn create_sink(&self, input_sample_rate: u32, input_channels: usize) -> Sink {
        Sink::new(
            input_sample_rate,
            input_channels,
            self.config.sample_rate().0,
            self.config.channels() as usize,
            self.output_buffer.clone(),
        )
    }

    fn playback_sample_rate(&self) -> usize {
        44100
    }

    fn frames_consumed(&self) -> u64 {
        self.frames_consumed.load(atomic::Ordering::SeqCst)
    }

    fn reset_frames_consumed(&self) {
        self.frames_consumed.store(0, atomic::Ordering::SeqCst);
    }

    fn play(&self) -> Result<(), AudioDeviceError> {
        Ok(())
    }

    fn stop(&self) -> Result<(), AudioDeviceError> {
        Ok(())
    }

    fn pause(&self) -> Result<(), AudioDeviceError> {
        Ok(())
    }

    fn healthcheck(&self) -> Result<(), AudioDeviceError> {
        Ok(())
    }
}

pub(super) struct CpalAudioDevice {
    _device: Device,
    config: SupportedStreamConfig,
    stream: Stream,
    output_buffer: Arc<Mutex<BoxAudioBuffer>>,
    frames_consumed: Arc<AtomicU64>,
    playing: AtomicBool,
    error_receiver: Receiver<AudioDeviceError>,
}

macro_rules! create_stream {
    (
        selected_format = $selected_format:expr,
        config = $cfg:ident,
        frames_consumed = $frames_consumed:ident,
        output_buffer = $buf:ident,
        device = $device:ident,
        stream_writer = $stream_writer:ident,
        error_callback = $error_callback:ident,
        supported_formats = [
            $($uf:ident => $lf:ident,)+
        ]
    ) => {
        match $selected_format {
            $(
                cpal::SampleFormat::$uf => $device.build_output_stream(
                    $cfg,
                    $stream_writer::<$lf>($cfg.channels as usize, $frames_consumed.clone(), $buf.clone()),
                    $error_callback,
                    None
                ),
            )+
            _ => unreachable!("unsupported sample format: {:?} (this is a bug)", $selected_format),
        }
    };
}

impl CpalAudioDevice {
    pub(super) fn new(
        preferred_output_device_name: Option<&str>,
    ) -> Result<Self, AudioDeviceError> {
        let host = cpal::default_host();
        let device = select_device(&host, preferred_output_device_name)?;
        log::info!("selected audio output device: {}", device.name()?);

        let supported_output_configs = device.supported_output_configs()?;
        let config = select_config(supported_output_configs)?
            .ok_or(AudioDeviceError::FailedToSelectConfig)?;
        log::info!(
            "selected audio output configuration: channels={}, sample_rate={}, sample_format={:?}",
            config.channels(),
            config.sample_rate().0,
            config.sample_format()
        );

        fn stream_writer<S: Sample + 'static>(
            channels: usize,
            frames_consumed: Arc<AtomicU64>,
            output_buffer: Arc<Mutex<BoxAudioBuffer>>,
        ) -> impl FnMut(&mut [S], &OutputCallbackInfo) + Send + 'static {
            move |data: &mut [S], info| {
                let mut output_buffer = output_buffer.lock().unwrap();
                write_audio_data(
                    channels,
                    frames_consumed.clone(),
                    &mut output_buffer,
                    data,
                    info,
                );
            }
        }

        let frames_consumed = Arc::new(AtomicU64::new(0));
        let output_buffer = Arc::new(Mutex::new(BoxAudioBuffer::empty(config.sample_format())));
        let (stream, error_receiver) = {
            let cfg = &config.config();
            let (err_tx, err_rx) = std::sync::mpsc::channel();
            let error_callback = move |err: StreamError| {
                log::error!("stream error: {}", err);
                if let Err(send_err) = err_tx.send(err.into()) {
                    log::error!("failed to send stream error to audio device: {}", send_err);
                }
            };
            let stream = create_stream!(
                selected_format = config.sample_format(),
                config = cfg,
                frames_consumed = frames_consumed,
                output_buffer = output_buffer,
                device = device,
                stream_writer = stream_writer,
                error_callback = error_callback,
                supported_formats = [
                    F32 => f32,
                    I16 => i16,
                    U16 => u16,
                    I32 => i32,
                    U32 => u32,
                    F64 => f64,
                    I8 => i8,
                    U8 => u8,
                ]
            );
            stream.map(|stream| (stream, err_rx))
        }?;
        stream.pause()?;

        Ok(Self {
            _device: device,
            config,
            stream,
            output_buffer,
            frames_consumed,
            playing: AtomicBool::new(false),
            error_receiver,
        })
    }
}

impl AudioDevice for CpalAudioDevice {
    fn create_sink(&self, input_sample_rate: u32, input_channels: usize) -> Sink {
        Sink::new(
            input_sample_rate,
            input_channels,
            self.config.sample_rate().0,
            self.config.channels() as usize,
            self.output_buffer.clone(),
        )
    }

    fn playback_sample_rate(&self) -> usize {
        self.config.sample_rate().0 as usize
    }

    fn frames_consumed(&self) -> u64 {
        self.frames_consumed.load(atomic::Ordering::SeqCst)
    }

    fn reset_frames_consumed(&self) {
        self.frames_consumed.store(0, atomic::Ordering::SeqCst)
    }

    fn stop(&self) -> Result<(), AudioDeviceError> {
        self.output_buffer.lock().unwrap().clear();
        self.pause()
    }

    fn play(&self) -> Result<(), AudioDeviceError> {
        self.stream.play()?;
        self.playing.store(true, atomic::Ordering::SeqCst);
        Ok(())
    }

    fn pause(&self) -> Result<(), AudioDeviceError> {
        self.stream.pause()?;
        self.playing.store(false, atomic::Ordering::SeqCst);
        Ok(())
    }

    fn healthcheck(&self) -> Result<(), AudioDeviceError> {
        if let Ok(err) = self.error_receiver.try_recv() {
            return Err(err);
        }
        Ok(())
    }
}

// TODO unit test
fn write_audio_data<S>(
    channels: usize,
    frames_consumed: Arc<AtomicU64>,
    output_buffer: &mut BoxAudioBuffer,
    data: &mut [S],
    _: &OutputCallbackInfo,
) where
    S: Sample + 'static,
{
    let output_buffer = output_buffer.expect_mut::<S>();
    frames_consumed.fetch_add(
        output_buffer.len() as u64 / channels as u64,
        atomic::Ordering::SeqCst,
    );
    let output_buffer_len = output_buffer.len();
    let source = output_buffer.drain(0..usize::min(output_buffer.len(), data.len()));
    for (from, into) in source.zip(data.iter_mut()) {
        *into = from;
    }
    let mut filled_in_silence = false;
    for into in data.iter_mut().skip(output_buffer_len) {
        *into = S::EQUILIBRIUM;
        filled_in_silence = true;
    }
    if filled_in_silence {
        static ONCE: OnceLock<()> = OnceLock::new();
        ONCE.get_or_init(|| {
            log::warn!(
                "filled output device with silence (this is either a performance issue or a bug)"
            );
        });
    }
}

fn select_device(host: &Host, preferred: Option<&str>) -> Result<Device, AudioDeviceError> {
    if let Some(preferred) = preferred {
        log::info!("looking for preferred audio device named \"{preferred}\"...");
    } else {
        log::info!("no preferred audio output device. Will attempt to use default.");
    }
    let devices = host
        .output_devices()
        .map_err(AudioDeviceError::FailedToQueryDevices)?;
    for device in devices {
        let device_name = device.name()?;
        log::info!("available audio output device: {device_name}");
        if preferred == Some(device_name.as_str()) {
            return Ok(device);
        }
    }
    host.default_output_device()
        .ok_or(AudioDeviceError::NoDefaultAudioOutputDevice)
}

fn select_config(
    supported_output_configs: impl Iterator<Item = SupportedStreamConfigRange>,
) -> Result<Option<SupportedStreamConfig>, AudioDeviceError> {
    let mut supported_output_configs = supported_output_configs.collect::<Vec<_>>();
    if supported_output_configs.is_empty() {
        return Ok(None);
    }

    supported_output_configs.sort_by(|a, b| {
        by_preferred_channels(a, b)
            .then_with(|| by_preferred_sample_rate(a, b))
            .then_with(|| by_preferred_sample_format(a, b))
    });
    log::info!("available audio output configurations in priority order:");
    for config in &supported_output_configs {
        log::info!(
            "  channels={}, sample_rate={}-{}, sample_format={:?}",
            config.channels(),
            config.min_sample_rate().0,
            config.max_sample_rate().0,
            config.sample_format()
        );
    }
    let selected = supported_output_configs.into_iter().next().unwrap();
    let selected = select_sample_rate(selected);
    if let cpal::SampleFormat::I64 | cpal::SampleFormat::U64 = selected.sample_format() {
        Err(AudioDeviceError::FailedToSelectConfig)
    } else {
        Ok(Some(selected))
    }
}

fn config_range_supports_sample_rate(range: &SupportedStreamConfigRange, sample_rate: u32) -> bool {
    range.min_sample_rate().0 <= sample_rate && range.max_sample_rate().0 >= sample_rate
}

fn select_sample_rate(range: SupportedStreamConfigRange) -> SupportedStreamConfig {
    for &hz in PREFERRED_SAMPLE_RATES {
        if config_range_supports_sample_rate(&range, hz) {
            return range.with_sample_rate(SampleRate(hz));
        }
    }
    range.with_max_sample_rate()
}

fn by_preferred_channels(
    left: &SupportedStreamConfigRange,
    right: &SupportedStreamConfigRange,
) -> Ordering {
    // Prefer two channels. Otherwise, maximize the number of channels.
    if left.channels() == right.channels() {
        Ordering::Equal
    } else if left.channels() == 2 {
        Ordering::Less
    } else if right.channels() == 2 {
        Ordering::Greater
    } else {
        right.channels().cmp(&left.channels())
    }
}

fn by_preferred_sample_rate(
    left: &SupportedStreamConfigRange,
    right: &SupportedStreamConfigRange,
) -> Ordering {
    // Sort preferred sample rates to the front
    for &hz in PREFERRED_SAMPLE_RATES {
        if config_range_supports_sample_rate(left, hz) {
            return Ordering::Less;
        }
        if config_range_supports_sample_rate(right, hz) {
            return Ordering::Greater;
        }
    }
    // Otherwise, choose the larger sample rate
    right.max_sample_rate().0.cmp(&left.max_sample_rate().0)
}

fn by_preferred_sample_format(
    left: &SupportedStreamConfigRange,
    right: &SupportedStreamConfigRange,
) -> Ordering {
    use cpal::SampleFormat as SF;
    for &format in &[
        // Preferred
        SF::F32,
        SF::I16,
        SF::U16,
        // These take more memory, but still retain quality
        SF::I32,
        SF::U32,
        SF::F64,
        // These lose quality
        SF::I8,
        SF::U8,
        // These aren't supported by Symphonia, so select against them
        SF::I64,
        SF::U64,
    ] {
        if left.sample_format() == format {
            return Ordering::Less;
        }
        if right.sample_format() == format {
            return Ordering::Greater;
        }
    }
    Ordering::Greater
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpal::{SampleFormat, SupportedBufferSize};

    #[test]
    fn preferred_channels() {
        fn cfg(channels: u16) -> SupportedStreamConfigRange {
            SupportedStreamConfigRange::new(
                channels,
                SampleRate(44100),
                SampleRate(44100),
                SupportedBufferSize::Unknown,
                SampleFormat::F32,
            )
        }

        let mut configs = [1, 2, 3, 4, 5, 6, 7, 8]
            .into_iter()
            .map(cfg)
            .collect::<Vec<_>>();

        fastrand::shuffle(&mut configs);
        configs.sort_by(by_preferred_channels);

        assert_eq!(
            vec![2, 8, 7, 6, 5, 4, 3, 1],
            configs
                .into_iter()
                .map(|c| c.channels())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn preferred_formats() {
        fn cfg(format: SampleFormat) -> SupportedStreamConfigRange {
            SupportedStreamConfigRange::new(
                1,
                SampleRate(44100),
                SampleRate(44100),
                SupportedBufferSize::Unknown,
                format,
            )
        }

        use SampleFormat::*;
        let mut configs = [I16, U16, I32, U32, F64, I8, U8, I64, U64, F32]
            .into_iter()
            .map(cfg)
            .collect::<Vec<_>>();

        fastrand::shuffle(&mut configs);
        configs.sort_by(by_preferred_sample_format);

        assert_eq!(
            vec![F32, I16, U16, I32, U32, F64, I8, U8, I64, U64],
            configs
                .into_iter()
                .map(|c| c.sample_format())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn preferred_sample_rates() {
        fn cfg(min_sample_rate: u32, max_sample_rate: u32) -> SupportedStreamConfigRange {
            SupportedStreamConfigRange::new(
                1,
                SampleRate(min_sample_rate),
                SampleRate(max_sample_rate),
                SupportedBufferSize::Unknown,
                SampleFormat::F32,
            )
        }
        fn shuffled_group(group: &[(u32, u32)]) -> Vec<SupportedStreamConfigRange> {
            let mut configs = group
                .iter()
                .map(|(min, max)| cfg(*min, *max))
                .collect::<Vec<_>>();
            fastrand::shuffle(&mut configs);
            configs
        }
        #[track_caller]
        fn test(values: &[(u32, u32)], expected: &[(u32, u32)]) {
            let mut configs = shuffled_group(values);
            configs.sort_by(by_preferred_sample_rate);
            assert_eq!(
                expected.to_vec(),
                configs
                    .into_iter()
                    .map(|c| (c.min_sample_rate().0, c.max_sample_rate().0))
                    .collect::<Vec<_>>()
            );
        }

        test(
            &[(8000, 16000), (96000, 96000)],
            &[(96000, 96000), (8000, 16000)],
        );
        test(
            &[(8000, 8000), (16000, 16000)],
            &[(16000, 16000), (8000, 8000)],
        );
        test(
            &[(8000, 8000), (96000, 96000), (44100, 44100), (44100, 48000)],
            &[(44100, 48000), (44100, 44100), (96000, 96000), (8000, 8000)],
        );
    }

    #[test]
    fn select_configs() {
        use SampleFormat::*;

        fn cfg(channels: u16, minmax: u32, format: SampleFormat) -> SupportedStreamConfigRange {
            SupportedStreamConfigRange::new(
                channels,
                SampleRate(minmax),
                SampleRate(minmax),
                SupportedBufferSize::Unknown,
                format,
            )
        }

        assert_eq!(None, select_config([].into_iter()).unwrap());

        assert_eq!(
            Some(cfg(2, 44100, F32).with_sample_rate(SampleRate(44100))),
            select_config([cfg(5, 44100, F32), cfg(2, 44100, F32), cfg(1, 44100, F32)].into_iter())
                .unwrap()
        );

        assert_eq!(
            Some(cfg(2, 48000, F32).with_sample_rate(SampleRate(48000))),
            select_config([cfg(2, 8000, F32), cfg(2, 96000, F32), cfg(2, 48000, F32)].into_iter())
                .unwrap()
        );

        assert_eq!(
            Some(cfg(2, 48000, I16).with_sample_rate(SampleRate(48000))),
            select_config([cfg(2, 48000, I8), cfg(2, 48000, U32), cfg(2, 48000, I16)].into_iter())
                .unwrap()
        );
    }
}
