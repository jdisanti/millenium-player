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

use self::sealed::BroadcastingAudioDevice;

use super::{
    sink::{AudioBuffer, BoxAudioBuffer, Sink},
    ChannelCount,
};
use crate::audio::SampleRate;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BuildStreamError, Device, DeviceNameError, Host, OutputCallbackInfo, PauseStreamError,
    PlayStreamError, Sample, SampleFormat, SizedSample, Stream, StreamError, SupportedStreamConfig,
    SupportedStreamConfigRange, SupportedStreamConfigsError,
};
use millenium_post_office::{
    broadcast::{BroadcastMessage, BroadcastSubscription, Broadcaster, Channel},
    types::Volume,
};
use std::{
    cmp::Ordering,
    fmt,
    sync::{
        atomic::{self, AtomicBool, AtomicU64, AtomicU8},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const PREFERRED_SAMPLE_RATES: &[u32] = &[48000, 44100, 88200, 96000];
const DESIRED_BUFFER_LENGTH: Duration = Duration::from_millis(500);

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

bitflags::bitflags! {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct AudioDeviceMessageChannel: u8 {
        const All = 0xFF;
        const Errors = 0x01;
        const Events = 0x02;
        const Requests = 0x04;
    }
}

impl Channel for AudioDeviceMessageChannel {
    fn matches(&self, other: Self) -> bool {
        self.bits() & other.bits() != 0
    }
}

#[derive(Clone, Debug)]
pub enum AudioDeviceMessage {
    Error(Arc<AudioDeviceError>),
    EventAudioDeviceIdle,
    EventPlaybackFinished,
    RequestAudioData,
}

impl BroadcastMessage for AudioDeviceMessage {
    type Channel = AudioDeviceMessageChannel;

    fn channel(&self) -> Self::Channel {
        match self {
            Self::Error(_) => AudioDeviceMessageChannel::Errors,
            Self::EventAudioDeviceIdle | Self::EventPlaybackFinished => {
                AudioDeviceMessageChannel::Events
            }
            Self::RequestAudioData => AudioDeviceMessageChannel::Requests,
        }
    }

    fn frequent(&self) -> bool {
        matches!(self, AudioDeviceMessage::RequestAudioData)
    }
}

/// Represents an output device that can play audio.
pub trait AudioDevice: BroadcastingAudioDevice {
    /// Create a sink for the given sample rate and number of channels.
    fn create_sink(&self, input_sample_rate: SampleRate, input_channels: ChannelCount) -> Sink;

    /// Returns the sample rate that playback occurs at.
    fn playback_sample_rate(&self) -> SampleRate;

    /// Returns the number of channels used for playback.
    fn playback_channels(&self) -> ChannelCount;

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

    /// Set the output volume on this device.
    fn set_volume(&self, volume: Volume);

    /// Returns the current output volume.
    fn volume(&self) -> Volume;

    /// Subscribe to this device's events.
    fn subscribe(
        &self,
        name: &'static str,
        channel: AudioDeviceMessageChannel,
    ) -> BroadcastSubscription<AudioDeviceMessage>;
}

mod sealed {
    use super::*;

    pub trait BroadcastingAudioDevice {
        /// Get the device's broadcaster.
        fn broadcaster(&self) -> Broadcaster<AudioDeviceMessage>;
    }
}

#[derive(thiserror::Error)]
#[error("failed to create audio device")]
pub struct CreateDeviceError {
    /// A fallback device that can't play audio, but keeps the application otherwise working.
    pub fallback_device: Box<dyn AudioDevice>,
    #[source]
    pub source: AudioDeviceError,
}

impl fmt::Debug for CreateDeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateDeviceError")
            .field("fallback_device", &"** present **")
            .field("source", &self.source)
            .finish()
    }
}

/// Create an audio device for this platform.
pub fn create_device(
    preferred_output_device_name: Option<&str>,
) -> Result<Box<dyn AudioDevice>, CreateDeviceError> {
    match CpalAudioDevice::new(preferred_output_device_name) {
        Ok(device) => Ok(Box::new(device)),
        Err(err) => {
            log::error!("failed to create cpal audio device: {}", err);
            Err(CreateDeviceError {
                fallback_device: Box::new(NullAudioDevice::new()),
                source: err,
            })
        }
    }
}

struct NullAudioDevice {
    config: SupportedStreamConfig,
    output_buffer: Arc<Mutex<BoxAudioBuffer>>,
    frames_consumed: AtomicU64,
    broadcaster: Broadcaster<AudioDeviceMessage>,
}

impl NullAudioDevice {
    pub(super) fn new() -> Self {
        Self {
            config: SupportedStreamConfig::new(
                2,
                cpal::SampleRate(44100),
                cpal::SupportedBufferSize::Unknown,
                SampleFormat::F32,
            ),
            output_buffer: Arc::new(Mutex::new(BoxAudioBuffer::new(
                SampleFormat::F32,
                AudioBuffer::new(Vec::<f32>::new()),
            ))),
            frames_consumed: AtomicU64::new(0),
            broadcaster: Broadcaster::new(),
        }
    }
}

impl BroadcastingAudioDevice for NullAudioDevice {
    fn broadcaster(&self) -> Broadcaster<AudioDeviceMessage> {
        self.broadcaster.clone()
    }
}

impl AudioDevice for NullAudioDevice {
    fn create_sink(&self, input_sample_rate: SampleRate, input_channels: ChannelCount) -> Sink {
        Sink::new(
            input_sample_rate,
            input_channels,
            self.config.sample_rate().0,
            self.config.channels() as ChannelCount,
            self.output_buffer.clone(),
            self.broadcaster.clone(),
        )
    }

    fn playback_sample_rate(&self) -> SampleRate {
        44100
    }

    fn playback_channels(&self) -> ChannelCount {
        2
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

    fn set_volume(&self, _volume: Volume) {}

    fn volume(&self) -> Volume {
        Volume::default()
    }

    fn subscribe(
        &self,
        name: &'static str,
        channel: AudioDeviceMessageChannel,
    ) -> BroadcastSubscription<AudioDeviceMessage> {
        self.broadcaster.subscribe(name, channel)
    }
}

#[derive(Default)]
struct StreamBuilder<'a> {
    config: Option<&'a SupportedStreamConfig>,
    frames_consumed: Option<Arc<AtomicU64>>,
    output_buffer: Option<Arc<Mutex<BoxAudioBuffer>>>,
    device: Option<&'a Device>,
    broadcaster: Option<Broadcaster<AudioDeviceMessage>>,
    volume: Option<Arc<AtomicU8>>,
}

impl<'a> StreamBuilder<'a> {
    fn new() -> Self {
        Default::default()
    }

    fn config(mut self, config: &'a SupportedStreamConfig) -> Self {
        self.config = Some(config);
        self
    }

    fn frames_consumed(mut self, frames_consumed: Arc<AtomicU64>) -> Self {
        self.frames_consumed = Some(frames_consumed);
        self
    }

    fn output_buffer(mut self, output_buffer: Arc<Mutex<BoxAudioBuffer>>) -> Self {
        self.output_buffer = Some(output_buffer);
        self
    }

    fn device(mut self, device: &'a Device) -> Self {
        self.device = Some(device);
        self
    }

    fn broadcaster(mut self, broadcaster: Broadcaster<AudioDeviceMessage>) -> Self {
        self.broadcaster = Some(broadcaster);
        self
    }

    fn volume(mut self, volume: Arc<AtomicU8>) -> Self {
        self.volume = Some(volume);
        self
    }

    fn output_stream<S>(&self) -> Result<Stream, BuildStreamError>
    where
        S: Sample + SizedSample + 'static,
        S::Float: From<f32>,
    {
        let config = self.config.expect("config required");
        let frames_consumed = self
            .frames_consumed
            .as_ref()
            .cloned()
            .expect("frames_consumed required");
        let output_buffer = self
            .output_buffer
            .as_ref()
            .cloned()
            .expect("output_buffer required");
        let device = self.device.expect("device required");
        let broadcaster = self
            .broadcaster
            .as_ref()
            .expect("broadcaster required")
            .clone();

        let desired_output_buffer_size =
            (DESIRED_BUFFER_LENGTH.as_secs_f32() * config.sample_rate().0 as f32) as usize;
        let channels = config.channels() as usize;
        let mut write_data_context = WriteAudioDataContext {
            channels,
            desired_output_buffer_size,
            broadcaster: broadcaster.clone(),
            frames_consumed,
            volume: self.volume.clone().expect("volume is required"),
            state: DeviceState::Idle,
        };
        let write_data = {
            move |data: &mut [S], _info: &OutputCallbackInfo| {
                let mut output_buffer = output_buffer.lock().unwrap();
                write_audio_data(&mut write_data_context, &mut output_buffer, data);
            }
        };

        let error_callback = move |err: StreamError| {
            log::error!("stream error: {}", err);
            broadcaster.broadcast(AudioDeviceMessage::Error(
                AudioDeviceError::from(err).into(),
            ));
        };
        device.build_output_stream(&config.config(), write_data, error_callback, None)
    }

    fn build(self) -> Result<Stream, BuildStreamError> {
        let config = self.config.expect("config required");

        let sample_format = config.sample_format();
        let stream = match sample_format {
            SampleFormat::F32 => self.output_stream::<f32>(),
            SampleFormat::F64 => self.output_stream::<f64>(),
            SampleFormat::I16 => self.output_stream::<i16>(),
            SampleFormat::I32 => self.output_stream::<i32>(),
            SampleFormat::I8 => self.output_stream::<i8>(),
            SampleFormat::U16 => self.output_stream::<u16>(),
            SampleFormat::U32 => self.output_stream::<u32>(),
            SampleFormat::U8 => self.output_stream::<u8>(),
            _ => unreachable!("unsupported sample format: {sample_format:?} (this is a bug)"),
        }?;
        Ok(stream)
    }
}

struct CpalAudioDevice {
    // Cpal audio structs
    _device: Device,
    config: SupportedStreamConfig,
    stream: Stream,

    // Information about the current state of playback
    frames_consumed: Arc<AtomicU64>,
    playing: AtomicBool,
    volume: Arc<AtomicU8>,

    // Audio data and message passing
    output_buffer: Arc<Mutex<BoxAudioBuffer>>,
    broadcaster: Broadcaster<AudioDeviceMessage>,
}

impl CpalAudioDevice {
    fn new(preferred_output_device_name: Option<&str>) -> Result<Self, AudioDeviceError> {
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

        let frames_consumed = Arc::new(AtomicU64::new(0));
        let output_buffer = Arc::new(Mutex::new(BoxAudioBuffer::empty(config.sample_format())));

        let broadcaster = Broadcaster::new();
        let volume = Arc::new(AtomicU8::new(Volume::default().into()));
        let stream = StreamBuilder::new()
            .config(&config)
            .device(&device)
            .broadcaster(broadcaster.clone())
            .frames_consumed(frames_consumed.clone())
            .output_buffer(output_buffer.clone())
            .volume(volume.clone())
            .build()?;

        stream.pause()?;

        Ok(Self {
            _device: device,
            config,
            stream,

            frames_consumed,
            playing: AtomicBool::new(false),
            volume,

            output_buffer,
            broadcaster,
        })
    }
}

impl BroadcastingAudioDevice for CpalAudioDevice {
    fn broadcaster(&self) -> Broadcaster<AudioDeviceMessage> {
        self.broadcaster.clone()
    }
}

impl AudioDevice for CpalAudioDevice {
    fn create_sink(&self, input_sample_rate: SampleRate, input_channels: ChannelCount) -> Sink {
        Sink::new(
            input_sample_rate,
            input_channels,
            self.config.sample_rate().0,
            self.config.channels() as ChannelCount,
            self.output_buffer.clone(),
            self.broadcaster.clone(),
        )
    }

    fn playback_sample_rate(&self) -> SampleRate {
        self.config.sample_rate().0 as SampleRate
    }

    fn playback_channels(&self) -> ChannelCount {
        self.config.channels() as ChannelCount
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
        log::info!("resumed audio device");
        Ok(())
    }

    fn pause(&self) -> Result<(), AudioDeviceError> {
        self.stream.pause()?;
        self.playing.store(false, atomic::Ordering::SeqCst);
        log::info!("paused audio device");
        Ok(())
    }

    fn set_volume(&self, volume: Volume) {
        self.volume.store(volume.into(), atomic::Ordering::Relaxed);
    }

    fn volume(&self) -> Volume {
        self.volume.load(atomic::Ordering::Relaxed).into()
    }

    fn subscribe(
        &self,
        name: &'static str,
        channel: AudioDeviceMessageChannel,
    ) -> BroadcastSubscription<AudioDeviceMessage> {
        self.broadcaster.subscribe(name, channel)
    }
}

#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
enum DeviceState {
    Idle,
    Playing,
    SilenceSince(Instant),
}

struct WriteAudioDataContext {
    channels: usize,
    desired_output_buffer_size: usize,
    broadcaster: Broadcaster<AudioDeviceMessage>,
    frames_consumed: Arc<AtomicU64>,
    volume: Arc<AtomicU8>,
    state: DeviceState,
}

fn write_audio_data<S>(
    WriteAudioDataContext {
        channels,
        desired_output_buffer_size,
        broadcaster,
        frames_consumed,
        volume,
        state,
    }: &mut WriteAudioDataContext,
    box_output_buffer: &mut BoxAudioBuffer,
    data: &mut [S],
) where
    S: Sample + 'static,
    S::Float: From<f32>,
{
    let output_buffer = box_output_buffer.expect_mut::<S>();
    if output_buffer.len() < *desired_output_buffer_size {
        broadcaster.broadcast(AudioDeviceMessage::RequestAudioData);
    }

    let len_to_consume = usize::min(output_buffer.len(), data.len());
    frames_consumed.fetch_add(
        len_to_consume as u64 / *channels as u64,
        atomic::Ordering::SeqCst,
    );
    let volume: <S as Sample>::Float = Volume::from(volume.load(atomic::Ordering::Relaxed))
        .as_percentage()
        .into();
    let source = output_buffer.drain(0..len_to_consume);
    for (from, into) in source.zip(data.iter_mut()) {
        *into = from.mul_amp(volume);
    }
    let mut filled_in_silence = false;
    for into in data.iter_mut().skip(len_to_consume) {
        *into = S::EQUILIBRIUM;
        filled_in_silence = true;
    }

    if !filled_in_silence {
        *state = DeviceState::Playing;
    }
    match state {
        DeviceState::Idle => {}
        DeviceState::Playing => {
            if filled_in_silence {
                broadcaster.broadcast(AudioDeviceMessage::EventPlaybackFinished);
                *state = DeviceState::SilenceSince(Instant::now());
            }
        }
        DeviceState::SilenceSince(start) => {
            if Instant::now() - *start >= Duration::from_secs(5) {
                broadcaster.broadcast(AudioDeviceMessage::EventAudioDeviceIdle);
                *state = DeviceState::Idle;
            }
        }
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
            return range.with_sample_rate(cpal::SampleRate(hz));
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
                cpal::SampleRate(44100),
                cpal::SampleRate(44100),
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
                cpal::SampleRate(44100),
                cpal::SampleRate(44100),
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
                cpal::SampleRate(min_sample_rate),
                cpal::SampleRate(max_sample_rate),
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
                cpal::SampleRate(minmax),
                cpal::SampleRate(minmax),
                SupportedBufferSize::Unknown,
                format,
            )
        }

        assert_eq!(None, select_config([].into_iter()).unwrap());

        assert_eq!(
            Some(cfg(2, 44100, F32).with_sample_rate(cpal::SampleRate(44100))),
            select_config([cfg(5, 44100, F32), cfg(2, 44100, F32), cfg(1, 44100, F32)].into_iter())
                .unwrap()
        );

        assert_eq!(
            Some(cfg(2, 48000, F32).with_sample_rate(cpal::SampleRate(48000))),
            select_config([cfg(2, 8000, F32), cfg(2, 96000, F32), cfg(2, 48000, F32)].into_iter())
                .unwrap()
        );

        assert_eq!(
            Some(cfg(2, 48000, I16).with_sample_rate(cpal::SampleRate(48000))),
            select_config([cfg(2, 48000, I8), cfg(2, 48000, U32), cfg(2, 48000, I16)].into_iter())
                .unwrap()
        );
    }

    #[test]
    fn write_audio_data_copy_data() {
        let mut output_buffer =
            BoxAudioBuffer::new(SampleFormat::F32, AudioBuffer::new(vec![128f32; 2000]));
        let frames_consumed = Arc::new(AtomicU64::new(0));
        let broadcaster = Broadcaster::new();
        let test_sub = broadcaster.subscribe("test", AudioDeviceMessageChannel::All);

        let mut output = vec![0f32; 1000];
        let mut context = WriteAudioDataContext {
            channels: 1,
            desired_output_buffer_size: 1000,
            broadcaster: broadcaster.clone(),
            frames_consumed,
            volume: Arc::new(AtomicU8::new(Volume::default().into())),
            state: DeviceState::Playing,
        };

        write_audio_data(&mut context, &mut output_buffer, &mut output);

        assert_eq!(
            1000,
            output_buffer.get_mut::<f32>().unwrap().len(),
            "it should drain 1000 samples from the output buffer"
        );
        assert!(
            output.iter().all(|&s| s == 128.0),
            "it should have copied the samples into the output"
        );
        assert_eq!(
            DeviceState::Playing,
            context.state,
            "it should remain in the playing state"
        );
        assert!(
            test_sub.try_recv().is_none(),
            "it should not broadcast any message"
        );
    }

    #[test]
    fn write_audio_data_copy_data_apply_volume() {
        let mut output_buffer =
            BoxAudioBuffer::new(SampleFormat::F32, AudioBuffer::new(vec![128f32; 2000]));
        let frames_consumed = Arc::new(AtomicU64::new(0));
        let broadcaster = Broadcaster::new();
        let test_sub = broadcaster.subscribe("test", AudioDeviceMessageChannel::All);

        let mut output = vec![0f32; 1000];
        let mut context = WriteAudioDataContext {
            channels: 1,
            desired_output_buffer_size: 1000,
            broadcaster: broadcaster.clone(),
            frames_consumed,
            volume: Arc::new(AtomicU8::new(Volume::from_percentage(0.5).into())),
            state: DeviceState::Playing,
        };

        write_audio_data(&mut context, &mut output_buffer, &mut output);

        assert_eq!(
            1000,
            output_buffer.get_mut::<f32>().unwrap().len(),
            "it should drain 1000 samples from the output buffer"
        );
        assert!(
            output.iter().all(|&s| s.round() == 64.0),
            "it should have copied the samples into the output at half volume"
        );
        assert_eq!(
            DeviceState::Playing,
            context.state,
            "it should remain in the playing state"
        );
        assert!(
            test_sub.try_recv().is_none(),
            "it should not broadcast any message"
        );
    }

    #[test]
    fn write_audio_data_request_more_audio() {
        let mut output_buffer =
            BoxAudioBuffer::new(SampleFormat::F32, AudioBuffer::new(vec![128f32; 2000]));
        let frames_consumed = Arc::new(AtomicU64::new(0));
        let broadcaster = Broadcaster::new();
        let test_sub = broadcaster.subscribe("test", AudioDeviceMessageChannel::All);

        let mut output = vec![0f32; 1000];
        let mut context = WriteAudioDataContext {
            channels: 1,
            desired_output_buffer_size: 3000,
            broadcaster: broadcaster.clone(),
            frames_consumed,
            volume: Arc::new(AtomicU8::new(Volume::default().into())),
            state: DeviceState::Playing,
        };

        write_audio_data(&mut context, &mut output_buffer, &mut output);

        assert_eq!(
            1000,
            output_buffer.get_mut::<f32>().unwrap().len(),
            "it should drain 1000 samples from the output buffer"
        );
        assert!(
            output.iter().all(|&s| s == 128.0),
            "it should have copied the samples into the output"
        );
        assert_eq!(
            DeviceState::Playing,
            context.state,
            "it should remain in the playing state"
        );
        assert!(
            matches!(
                test_sub.try_recv().unwrap(),
                AudioDeviceMessage::RequestAudioData
            ),
            "it should request audio data"
        );
    }

    #[test]
    fn write_audio_data_playback_finished() {
        let mut output_buffer =
            BoxAudioBuffer::new(SampleFormat::F32, AudioBuffer::new(vec![128f32; 500]));
        let frames_consumed = Arc::new(AtomicU64::new(0));
        let broadcaster = Broadcaster::new();
        let test_sub = broadcaster.subscribe("test", AudioDeviceMessageChannel::All);

        let mut output = vec![123f32; 1000];
        let mut context = WriteAudioDataContext {
            channels: 1,
            desired_output_buffer_size: 3000,
            broadcaster: broadcaster.clone(),
            frames_consumed,
            volume: Arc::new(AtomicU8::new(Volume::default().into())),
            state: DeviceState::Playing,
        };

        write_audio_data(&mut context, &mut output_buffer, &mut output);

        assert_eq!(
            0,
            output_buffer.get_mut::<f32>().unwrap().len(),
            "it should drain all the remaining samples from the output buffer"
        );
        assert!(
            output.iter().take(500).all(|&s| s == 128.0),
            "it should have copied the 500 samples into the output"
        );
        assert!(
            output.iter().skip(500).all(|&s| s == 0.0),
            "it should have filled the remaining with silence"
        );
        assert!(
            matches!(context.state, DeviceState::SilenceSince(_)),
            "it should switch to the SilenceSince state"
        );
        assert!(
            matches!(
                test_sub.try_recv().unwrap(),
                AudioDeviceMessage::RequestAudioData
            ),
            "it should always request audio data when it needs more"
        );
        assert!(
            matches!(
                test_sub.try_recv().unwrap(),
                AudioDeviceMessage::EventPlaybackFinished
            ),
            "it should broadcast that playback is finished"
        );
    }

    #[test]
    fn write_audio_data_idle_device() {
        let mut output_buffer =
            BoxAudioBuffer::new(SampleFormat::F32, AudioBuffer::new(Vec::<f32>::new()));
        let frames_consumed = Arc::new(AtomicU64::new(0));
        let broadcaster = Broadcaster::new();
        let test_sub = broadcaster.subscribe("test", AudioDeviceMessageChannel::All);

        let mut output = vec![123f32; 1000];
        let mut context = WriteAudioDataContext {
            channels: 1,
            desired_output_buffer_size: 3000,
            broadcaster: broadcaster.clone(),
            frames_consumed,
            volume: Arc::new(AtomicU8::new(Volume::default().into())),
            state: DeviceState::SilenceSince(Instant::now() - Duration::from_secs(10)),
        };

        write_audio_data(&mut context, &mut output_buffer, &mut output);

        assert!(
            output.iter().all(|&s| s == 0.0),
            "it should have filled the output with silence"
        );
        assert!(
            matches!(context.state, DeviceState::Idle),
            "it should switch to the Idle state"
        );
        assert!(
            matches!(
                test_sub.try_recv().unwrap(),
                AudioDeviceMessage::RequestAudioData
            ),
            "it should always request audio data when it needs more"
        );
        assert!(
            matches!(
                test_sub.try_recv().unwrap(),
                AudioDeviceMessage::EventAudioDeviceIdle
            ),
            "it should broadcast that the device is idle"
        );
    }

    #[test]
    fn write_audio_data_idle_back_to_playing() {
        let mut output_buffer =
            BoxAudioBuffer::new(SampleFormat::F32, AudioBuffer::new(vec![128f32; 3000]));
        let frames_consumed = Arc::new(AtomicU64::new(0));
        let broadcaster = Broadcaster::new();
        let test_sub = broadcaster.subscribe("test", AudioDeviceMessageChannel::All);

        let mut output = vec![123f32; 1000];
        let mut context = WriteAudioDataContext {
            channels: 1,
            desired_output_buffer_size: 3000,
            broadcaster: broadcaster.clone(),
            frames_consumed,
            volume: Arc::new(AtomicU8::new(Volume::default().into())),
            state: DeviceState::Idle,
        };

        write_audio_data(&mut context, &mut output_buffer, &mut output);

        assert!(
            output.iter().all(|&s| s == 128.0),
            "it should have copied the samples into the output"
        );
        assert!(
            matches!(context.state, DeviceState::Playing),
            "it should switch to the Playing state"
        );
        assert!(
            test_sub.try_recv().is_none(),
            "it shouldn't broadcast a message"
        );
    }
}
