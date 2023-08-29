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

use super::source::SourceBuffer;
use cpal::{Sample, SampleFormat};
use rubato::{SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};
use std::{
    any::Any,
    mem,
    ops::RangeBounds,
    sync::{mpsc::Receiver, Arc, Mutex},
    time::Duration,
};

const CHUNK_SIZE_FRAMES: usize = 1024;
const DESIRED_QUEUE_LENGTH: Duration = Duration::from_millis(100);

pub(super) struct Sink {
    input_sample_rate: u32,
    input_channels: usize,
    output_sample_rate: u32,
    output_channels: usize,
    desired_input_frames: usize,
    resampler: Option<SincFixedIn<f32>>,
    input_buffer: Arc<Mutex<SourceBuffer>>,
    output_buffer: Arc<Mutex<BoxAudioBuffer>>,
    output_needed_signal: Arc<Receiver<()>>,
}

impl Sink {
    pub(crate) fn new(
        input_sample_rate: u32,
        input_channels: usize,
        output_sample_rate: u32,
        output_channels: usize,
        output_buffer: Arc<Mutex<BoxAudioBuffer>>,
        output_needed_signal: Arc<Receiver<()>>,
    ) -> Self {
        let resampler = if input_sample_rate != output_sample_rate {
            Some(
                SincFixedIn::new(
                    // Resample ratio (into / from)
                    output_sample_rate as f64 / input_sample_rate as f64,
                    // Max relative resample ratio
                    12.0,
                    SincInterpolationParameters {
                        sinc_len: 256,
                        f_cutoff: 0.95,
                        interpolation: SincInterpolationType::Linear,
                        oversampling_factor: 256,
                        window: WindowFunction::BlackmanHarris2,
                    },
                    CHUNK_SIZE_FRAMES,
                    output_channels,
                )
                .expect("failed to create resampler (this is a bug)"),
            )
        } else {
            None
        };
        Self {
            input_sample_rate,
            input_channels,
            output_sample_rate,
            output_channels,
            desired_input_frames: (DESIRED_QUEUE_LENGTH.as_secs_f32() * input_sample_rate as f32)
                as usize,
            resampler,
            input_buffer: Arc::new(Mutex::new(SourceBuffer::empty(
                input_sample_rate,
                input_channels,
            ))),
            output_buffer,
            output_needed_signal,
        }
    }

    pub(crate) fn input_sample_rate(&self) -> u32 {
        self.input_sample_rate
    }

    pub(crate) fn input_channels(&self) -> usize {
        self.input_channels
    }

    pub(crate) fn needs_more_chunks(&self) -> bool {
        self.input_buffer.lock().unwrap().frame_count() < self.desired_input_frames
    }

    fn convert_chunk(
        resampler: Option<&mut SincFixedIn<f32>>,
        output_sample_rate: u32,
        output_channels: usize,
        chunk: SourceBuffer,
    ) -> SourceBuffer {
        let chunk = chunk.remix(output_channels);
        if let Some(resampler) = resampler {
            chunk.resampled(output_sample_rate, resampler)
        } else {
            chunk
        }
    }

    /// This is a blocking call that sends data to the audio device as needed.
    pub(crate) fn send_audio_with_timeout(&mut self, timeout: Duration) {
        if self.output_needed_signal.recv_timeout(timeout).is_ok() {
            let mut input_buffer = self.input_buffer.lock().unwrap();
            if input_buffer.frame_count() >= CHUNK_SIZE_FRAMES {
                let chunk = Self::convert_chunk(
                    self.resampler.as_mut(),
                    self.output_sample_rate,
                    self.output_channels,
                    input_buffer.drain(CHUNK_SIZE_FRAMES),
                );

                let mut output_buffer = self.output_buffer.lock().unwrap();
                output_buffer.extend(&chunk);
            } else {
                log::warn!("not decoding audio fast enough to satisfy output device",);
            }
        }
    }

    pub(crate) fn queue(&mut self, source: SourceBuffer) {
        // The sink needs to be recreated if the sample rate or number of channels changes
        debug_assert!(source.sample_rate() == self.input_sample_rate);
        debug_assert!(source.channel_count() == self.input_channels);

        let mut input_buffer = self.input_buffer.lock().unwrap();
        input_buffer.extend(source);
    }

    pub(crate) fn flush(&mut self) {
        let mut input_buffer = self.input_buffer.lock().unwrap();
        if input_buffer.frame_count() == 0 {
            return;
        }

        let mut chunk =
            SourceBuffer::empty(input_buffer.sample_rate(), input_buffer.channel_count());
        mem::swap(&mut chunk, &mut input_buffer);

        chunk.extend_with_silence(CHUNK_SIZE_FRAMES);
        let chunk = Self::convert_chunk(
            self.resampler.as_mut(),
            self.output_sample_rate,
            self.output_channels,
            chunk,
        );

        let mut output_buffer = self.output_buffer.lock().unwrap();
        output_buffer.extend(&chunk);
        let _ = (output_buffer, chunk);

        input_buffer.clear();
    }
}

pub(super) struct AudioBuffer<S> {
    data: Vec<S>,
}

impl<S> AudioBuffer<S> {
    pub(super) fn new(data: Vec<S>) -> Self {
        Self { data }
    }

    pub(super) fn clear(&mut self) {
        self.data.clear();
    }

    pub(super) fn drain<R>(&mut self, range: R) -> impl Iterator<Item = S> + '_
    where
        R: RangeBounds<usize>,
    {
        self.data.drain(range)
    }

    pub(super) fn len(&self) -> usize {
        self.data.len()
    }
}

impl<S> AudioBuffer<S>
where
    S: symphonia::core::sample::Sample,
    S: symphonia::core::conv::FromSample<f32>,
{
    fn extend(&mut self, source: &SourceBuffer) {
        debug_assert!(source.channel_count() > 0);

        source.extend_interleaved_into(&mut self.data);
    }
}

pub(super) struct BoxAudioBuffer {
    format: SampleFormat,
    inner: Box<dyn Any + Send>,
}

impl BoxAudioBuffer {
    pub(super) fn new<S: Sample + Send + 'static>(
        format: SampleFormat,
        buffer: AudioBuffer<S>,
    ) -> Self {
        Self {
            format,
            inner: Box::new(buffer),
        }
    }

    pub(super) fn empty(format: SampleFormat) -> Self {
        Self {
            format,
            inner: match format {
                SampleFormat::F32 => Box::new(AudioBuffer::<f32>::new(Vec::new())),
                SampleFormat::F64 => Box::new(AudioBuffer::<f64>::new(Vec::new())),
                SampleFormat::I16 => Box::new(AudioBuffer::<i16>::new(Vec::new())),
                SampleFormat::I32 => Box::new(AudioBuffer::<i32>::new(Vec::new())),
                SampleFormat::I8 => Box::new(AudioBuffer::<i8>::new(Vec::new())),
                SampleFormat::U16 => Box::new(AudioBuffer::<u16>::new(Vec::new())),
                SampleFormat::U32 => Box::new(AudioBuffer::<u32>::new(Vec::new())),
                SampleFormat::U8 => Box::new(AudioBuffer::<u8>::new(Vec::new())),
                SampleFormat::I64 | SampleFormat::U64 => {
                    unreachable!("unsupported: {}", format)
                }
                _ => unreachable!("{}", format),
            },
        }
    }

    pub(super) fn extend(&mut self, source: &SourceBuffer) {
        match self.format {
            SampleFormat::F32 => self.expect_mut::<f32>().extend(source),
            SampleFormat::F64 => self.expect_mut::<f64>().extend(source),
            SampleFormat::I16 => self.expect_mut::<i16>().extend(source),
            SampleFormat::I32 => self.expect_mut::<i32>().extend(source),
            SampleFormat::I8 => self.expect_mut::<i8>().extend(source),
            SampleFormat::U16 => self.expect_mut::<u16>().extend(source),
            SampleFormat::U32 => self.expect_mut::<u32>().extend(source),
            SampleFormat::U8 => self.expect_mut::<u8>().extend(source),
            SampleFormat::I64 | SampleFormat::U64 => unreachable!("unsupported: {}", self.format),
            _ => unreachable!("{}", self.format),
        }
    }

    pub(super) fn clear(&mut self) {
        match self.format {
            SampleFormat::F32 => self.expect_mut::<f32>().clear(),
            SampleFormat::F64 => self.expect_mut::<f64>().clear(),
            SampleFormat::I16 => self.expect_mut::<i16>().clear(),
            SampleFormat::I32 => self.expect_mut::<i32>().clear(),
            SampleFormat::I8 => self.expect_mut::<i8>().clear(),
            SampleFormat::U16 => self.expect_mut::<u16>().clear(),
            SampleFormat::U32 => self.expect_mut::<u32>().clear(),
            SampleFormat::U8 => self.expect_mut::<u8>().clear(),
            SampleFormat::I64 | SampleFormat::U64 => unreachable!("unsupported: {}", self.format),
            _ => unreachable!("{}", self.format),
        }
    }

    #[inline]
    pub(super) fn get_mut<S: Sample + 'static>(&mut self) -> Option<&mut AudioBuffer<S>> {
        self.inner.downcast_mut::<AudioBuffer<S>>()
    }
    #[inline]
    pub(super) fn expect_mut<S: Sample + 'static>(&mut self) -> &mut AudioBuffer<S> {
        self.get_mut().expect("failed to downcast audio buffer")
    }
}
