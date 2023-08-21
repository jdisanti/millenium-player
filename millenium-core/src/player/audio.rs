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

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BuildStreamError, Device, DeviceNameError, Host, OutputCallbackInfo, PlayStreamError, Sample,
    SampleRate, Stream, StreamError, SupportedStreamConfig, SupportedStreamConfigRange,
    SupportedStreamConfigsError,
};
use std::{
    any::Any,
    collections::VecDeque,
    sync::{mpsc::Receiver, Arc, Mutex},
    time::Duration,
};
use symphonia_core::audio::{
    AudioBuffer as SymphoniaAudioBuffer, AudioBufferRef as SymphoniaAudioBufferRef, Signal as _,
};
use symphonia_core::sample::Sample as SymphoniaSample;

const PREFERRED_SAMPLE_RATE: SampleRate = SampleRate(44100);
const DESIRED_QUEUE_LENGTH: Duration = Duration::from_millis(100);

#[derive(Debug, thiserror::Error)]
pub(super) enum AudioDeviceError {
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
}

struct AudioBuffer<S> {
    data: Vec<S>,
    duration: Duration,
    next: usize,
}

impl<S> AudioBuffer<S> {
    fn new(data: Vec<S>, duration: Duration) -> Self {
        debug_assert!(!data.is_empty());
        Self {
            data,
            duration,
            next: 0,
        }
    }
}
impl<S> AudioBuffer<S>
where
    S: Sample + SymphoniaSample,
{
    fn from(channels: u32, symphonia_buf: SymphoniaAudioBuffer<S>) -> Self {
        let symphonia_channels = symphonia_buf.spec().channels.count();
        debug_assert!(symphonia_channels > 0);

        let n = symphonia_buf.frames();
        debug_assert!(channels > 0 && n > 0);

        let mut buffer = vec![S::EQUILIBRIUM; n * channels as usize];
        for channel in 0..channels as usize {
            let chan = symphonia_buf.chan(usize::min(channel, symphonia_channels - 1));
            let mut buf_iter = buffer.iter_mut().skip(channel).step_by(channels as usize);
            for sample in chan {
                *buf_iter.next().unwrap() = *sample;
            }
        }
        let duration = Duration::from_secs_f64(n as f64 / symphonia_buf.spec().rate as f64);
        Self::new(buffer, duration)
    }

    #[inline]
    fn next(&mut self) -> Option<S> {
        let sample = self.data.get(self.next).copied();
        self.next = self.next.saturating_add(1);
        sample
    }
}

struct BoxAudioBuffer {
    inner: Box<dyn Any + Send>,
    duration: Duration,
}

impl BoxAudioBuffer {
    fn new<S: Sample + SymphoniaSample + Send + 'static>(buffer: AudioBuffer<S>) -> Self {
        Self {
            duration: buffer.duration,
            inner: Box::new(buffer),
        }
    }

    #[inline]
    fn get_mut<S: Sample + SymphoniaSample + 'static>(&mut self) -> Option<&mut AudioBuffer<S>> {
        self.inner.downcast_mut::<AudioBuffer<S>>()
    }
    #[inline]
    fn expect_mut<S: Sample + SymphoniaSample + 'static>(&mut self) -> &mut AudioBuffer<S> {
        self.get_mut().expect("failed to downcast audio buffer")
    }
}

pub(super) struct AudioDevice {
    _device: Device,
    _config: SupportedStreamConfig,
    stream: Stream,
    buffers: Arc<Mutex<VecDeque<BoxAudioBuffer>>>,
    error_receiver: Receiver<AudioDeviceError>,
}

macro_rules! create_stream {
    (
        selected_format = $selected_format:expr,
        config = $cfg:ident,
        buffers = $buf:ident,
        device = $device:ident,
        stream_writer = $stream_writer:ident,
        error_callback = $error_callback:ident,
        supported_formats = [
            $($uf:ident => $lf:ident,)+
        ]
    ) => {
        match $selected_format {
            $(cpal::SampleFormat::$uf =>
                $device.build_output_stream($cfg, $stream_writer::<$lf>($buf.clone()), $error_callback, None),)+
            _ => unreachable!("unsupported sample format: {:?} (this is a bug)", $selected_format),
        }
    };
}

impl AudioDevice {
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

        let buffers = Arc::new(Mutex::new(VecDeque::new()));
        fn stream_writer<S: Sample + SymphoniaSample + 'static>(
            buffers: Arc<Mutex<VecDeque<BoxAudioBuffer>>>,
        ) -> impl FnMut(&mut [S], &OutputCallbackInfo) + Send + 'static {
            move |data: &mut [S], info| {
                let mut buffers = buffers.lock().unwrap();
                write_audio_data(&mut buffers, data, info);
            }
        }

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
                buffers = buffers,
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
        stream.play()?;

        Ok(Self {
            _device: device,
            _config: config,
            stream,
            buffers,
            error_receiver,
        })
    }

    fn queued_duration(&self) -> Duration {
        let buffers = self.buffers.lock().unwrap();
        buffers.iter().map(|buf| buf.duration).sum()
    }

    pub(super) fn needs_more_chunks(&self) -> bool {
        self.queued_duration() < DESIRED_QUEUE_LENGTH
    }

    pub(super) fn queue_chunk(&self, chunk: &SymphoniaAudioBufferRef<'_>) {
        let converted = convert_audio(&self._config, chunk);
        let mut buffers = self.buffers.lock().unwrap();
        buffers.push_back(converted);
    }

    #[allow(dead_code)] // TODO remove once used
    pub(super) fn stop(&self) {
        let mut buffers = self.buffers.lock().unwrap();
        buffers.clear();
    }

    pub(super) fn play(&self) {
        // TODO: error handling
        self.stream.play().unwrap();
    }

    pub(super) fn pause(&self) {
        // TODO: error handling
        self.stream.pause().unwrap();
    }

    pub(super) fn healthcheck(&self) -> Result<(), AudioDeviceError> {
        if let Ok(err) = self.error_receiver.try_recv() {
            return Err(err);
        }
        Ok(())
    }
}

// TODO unit test
fn write_audio_data<S: Sample + SymphoniaSample + 'static>(
    buffers: &mut VecDeque<BoxAudioBuffer>,
    data: &mut [S],
    _: &OutputCallbackInfo,
) {
    for sample in data.iter_mut() {
        let next = loop {
            if let Some(buffer) = buffers.front_mut() {
                let buffer = buffer.expect_mut::<S>();
                if let Some(value) = buffer.next() {
                    break Some(value);
                } else {
                    buffers.pop_front();
                }
            } else {
                break None;
            }
        };
        *sample = next.unwrap_or(S::EQUILIBRIUM);
    }
}

macro_rules! convert_format {
    ($channels:expr, $into:expr, $from:ident, supported_formats = [$($uf:ident => $lf:ident,)+]) => {
        match $into {
            $(cpal::SampleFormat::$uf => {
                let mut buf = $from.make_equivalent::<$lf>();
                $from.convert(&mut buf);
                BoxAudioBuffer::new(AudioBuffer::from($channels, buf))
            },)+
            _ => unreachable!("unsupported sample format: {:?} (this is a bug)", $into),
        }
    };
}

fn convert_audio(
    into_format: &SupportedStreamConfig,
    from: &SymphoniaAudioBufferRef<'_>,
) -> BoxAudioBuffer {
    if into_format.sample_rate().0 == from.spec().rate {
        convert_format!(into_format.channels().into(), into_format.sample_format(), from, supported_formats = [
            F32 => f32,
            I16 => i16,
            U16 => u16,
            I32 => i32,
            U32 => u32,
            F64 => f64,
            I8 => i8,
            U8 => u8,
        ])
    } else {
        // TODO: sample rate conversion
        unimplemented!("sample rate conversion")
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
    let mut current_selection = None;
    for config in supported_output_configs {
        log::info!(
            "available output configuration: channels={}, sample_rate={}-{}, sample_format={:?}",
            config.channels(),
            config.min_sample_rate().0,
            config.max_sample_rate().0,
            config.sample_format()
        );
        if config.channels() == 2
            && config.min_sample_rate() <= PREFERRED_SAMPLE_RATE
            && config.max_sample_rate() >= PREFERRED_SAMPLE_RATE
        {
            if let Some(original) = current_selection.take() {
                current_selection = Some(preferred_sample_format(original, config));
            } else {
                current_selection = Some(config);
            }
        }
    }
    if let Some(selection) = &current_selection {
        if let cpal::SampleFormat::I64 | cpal::SampleFormat::U64 = selection.sample_format() {
            return Err(AudioDeviceError::FailedToSelectConfig);
        }
    }
    Ok(current_selection.map(|selection| selection.with_sample_rate(PREFERRED_SAMPLE_RATE)))
}

fn preferred_sample_format(
    left: SupportedStreamConfigRange,
    right: SupportedStreamConfigRange,
) -> SupportedStreamConfigRange {
    use cpal::SampleFormat as SF;
    match (left.sample_format(), right.sample_format()) {
        // Preferred
        (SF::F32, _) => left,
        (_, SF::F32) => right,
        (SF::I16, _) => left,
        (_, SF::I16) => right,
        (SF::U16, _) => left,
        (_, SF::U16) => right,
        // These take more memory, but still retain quality
        (SF::I32, _) => left,
        (_, SF::I32) => right,
        (SF::U32, _) => left,
        (_, SF::U32) => right,
        (SF::F64, _) => left,
        (_, SF::F64) => right,
        // These lose quality
        (SF::I8, _) => left,
        (_, SF::I8) => right,
        (SF::U8, _) => left,
        (_, SF::U8) => right,
        // These aren't supported by Symphonia, so select against them
        (SF::I64, _) => right,
        (_, SF::I64) => left,
        (SF::U64, _) => right,
        (_, SF::U64) => left,
        // SampleFormat is non-exhaustive
        _ => left,
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use cpal::{SampleFormat, SupportedBufferSize};
    use symphonia_core::audio::{Channels, SignalSpec};

    use super::*;

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
        for other in &[I16, U16, I32, U32, F64, I8, U8, I64, U64] {
            assert_eq!(cfg(F32), preferred_sample_format(cfg(F32), cfg(*other)));
            assert_eq!(cfg(F32), preferred_sample_format(cfg(*other), cfg(F32)));
        }
        for other in &[U16, I32, U32, F64, I8, U8, I64, U64] {
            assert_eq!(cfg(I16), preferred_sample_format(cfg(I16), cfg(*other)));
            assert_eq!(cfg(I16), preferred_sample_format(cfg(*other), cfg(I16)));
        }
        for other in &[I32, U32, F64, I8, U8, I64, U64] {
            assert_eq!(cfg(U16), preferred_sample_format(cfg(U16), cfg(*other)));
            assert_eq!(cfg(U16), preferred_sample_format(cfg(*other), cfg(U16)));
        }
        for other in &[U32, F64, I8, U8, I64, U64] {
            assert_eq!(cfg(I32), preferred_sample_format(cfg(I32), cfg(*other)));
            assert_eq!(cfg(I32), preferred_sample_format(cfg(*other), cfg(I32)));
        }
        for other in &[F64, I8, U8, I64, U64] {
            assert_eq!(cfg(U32), preferred_sample_format(cfg(U32), cfg(*other)));
            assert_eq!(cfg(U32), preferred_sample_format(cfg(*other), cfg(U32)));
        }
        for other in &[F64, I8, U8, I64, U64] {
            assert_eq!(cfg(F64), preferred_sample_format(cfg(F64), cfg(*other)));
            assert_eq!(cfg(F64), preferred_sample_format(cfg(*other), cfg(F64)));
        }
        for other in &[U8, I64, U64] {
            assert_eq!(cfg(I8), preferred_sample_format(cfg(I8), cfg(*other)));
            assert_eq!(cfg(I8), preferred_sample_format(cfg(*other), cfg(I8)));
        }
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
            select_config([cfg(2, 44100, F32)].into_iter()).unwrap()
        );

        assert_eq!(
            Some(cfg(2, 44100, F32).with_sample_rate(SampleRate(44100))),
            select_config([cfg(2, 48000, F32), cfg(2, 44100, F32)].into_iter(),).unwrap()
        );

        assert_eq!(
            Some(cfg(2, 44100, I16).with_sample_rate(SampleRate(44100))),
            select_config([cfg(2, 48000, F32), cfg(2, 44100, I16)].into_iter(),).unwrap()
        );

        assert_eq!(
            None,
            select_config([cfg(2, 48000, F32), cfg(2, 96000, I16)].into_iter(),).unwrap()
        );
    }

    #[test]
    fn convert_sample_format_f32_to_i16_mono() {
        let into = SupportedStreamConfig::new(
            1,
            SampleRate(44100),
            SupportedBufferSize::Unknown,
            SampleFormat::I16,
        );

        let mut from = SymphoniaAudioBuffer::new(20, SignalSpec::new(44100, Channels::FRONT_LEFT));
        from.render(None, |planes, n| {
            for plane in planes.planes() {
                plane[n] = 0.5;
            }
            Ok(())
        })
        .unwrap();

        let mut out = convert_audio(&into, &SymphoniaAudioBufferRef::F32(Cow::Owned(from)));
        let out = out.expect_mut::<i16>();
        assert!(out
            .data
            .iter()
            .all(|&s| dbg!(s) == dbg!((0.5 * i16::MAX as f32) as i16 + 1)));
        assert_eq!(20, out.data.len());
    }

    #[test]
    fn convert_sample_format_i16_to_f32_stereo() {
        let into = SupportedStreamConfig::new(
            2,
            SampleRate(44100),
            SupportedBufferSize::Unknown,
            SampleFormat::F32,
        );

        let mut from: SymphoniaAudioBuffer<i16> = SymphoniaAudioBuffer::new(
            20,
            SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT),
        );
        from.render(None, |planes, n| {
            for (i, plane) in planes.planes().iter_mut().enumerate() {
                plane[n] = ((0.2 * (i + 1) as f32) * i16::MAX as f32) as i16;
            }
            Ok(())
        })
        .unwrap();

        let mut out = convert_audio(&into, &SymphoniaAudioBufferRef::S16(Cow::Owned(from)));
        let out = out.expect_mut::<f32>();
        for chunk in out.data.chunks_exact(2) {
            assert_eq!((chunk[0] * 10.0).round() as i32, 2);
            assert_eq!((chunk[1] * 10.0).round() as i32, 4);
        }
        assert_eq!(40, out.data.len());
    }

    #[test]
    fn convert_sample_format_i16_to_f32_stereo_to_mono() {
        let into = SupportedStreamConfig::new(
            1,
            SampleRate(44100),
            SupportedBufferSize::Unknown,
            SampleFormat::F32,
        );

        let mut from: SymphoniaAudioBuffer<i16> = SymphoniaAudioBuffer::new(
            20,
            SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT),
        );
        from.render(None, |planes, n| {
            for (i, plane) in planes.planes().iter_mut().enumerate() {
                plane[n] = ((0.2 * (i + 1) as f32) * i16::MAX as f32) as i16;
            }
            Ok(())
        })
        .unwrap();

        let mut out = convert_audio(&into, &SymphoniaAudioBufferRef::S16(Cow::Owned(from)));
        let out = out.expect_mut::<f32>();
        assert!(out.data.iter().all(|s| (s * 10.0).round() as i32 == 2));
        assert_eq!(20, out.data.len());
    }

    #[test]
    fn convert_sample_format_i16_to_i32_mono_to_stereo() {
        let into = SupportedStreamConfig::new(
            2,
            SampleRate(44100),
            SupportedBufferSize::Unknown,
            SampleFormat::I32,
        );

        let mut from: SymphoniaAudioBuffer<i16> = SymphoniaAudioBuffer::new(
            20,
            SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT),
        );
        from.render(None, |planes, n| {
            for plane in planes.planes() {
                plane[n] = 1;
            }
            Ok(())
        })
        .unwrap();

        let mut out = convert_audio(&into, &SymphoniaAudioBufferRef::S16(Cow::Owned(from)));
        let out = out.expect_mut::<i32>();
        assert!(out.data.iter().all(|&s| dbg!(s) == 65536));
        assert_eq!(40, out.data.len());
    }
}
