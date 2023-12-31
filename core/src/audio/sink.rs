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

use super::{
    device::{AudioDeviceMessage, AudioDeviceMessageChannel},
    source::SourceBuffer,
    ChannelCount, SampleRate,
};
use cpal::{Sample, SampleFormat};
use millenium_post_office::broadcast::{BroadcastSubscription, Broadcaster};
use rubato::{FftFixedInOut, Resampler};
use std::{
    any::Any,
    cell::RefCell,
    ops::RangeBounds,
    sync::{Arc, Mutex},
    time::Duration,
};

const DESIRED_CHUNK_SIZE_FRAMES: usize = 2048;
const DESIRED_QUEUE_LENGTH: Duration = Duration::from_millis(500);

struct ResampleBuffers {
    input: SourceBuffer,
    output: SourceBuffer,
}

/// A sink for audio data that sends that data to the audio device.
pub struct Sink {
    input_sample_rate: SampleRate,
    input_channels: ChannelCount,
    output_sample_rate: SampleRate,
    output_channels: ChannelCount,
    desired_input_frames: usize,
    chunk_size_frames: usize,
    resampler: Option<RefCell<FftFixedInOut<f32>>>,
    resample_buffers: RefCell<ResampleBuffers>,
    input_buffer: Arc<Mutex<SourceBuffer>>,
    output_buffer: Arc<Mutex<BoxAudioBuffer>>,
    subscription: BroadcastSubscription<AudioDeviceMessage>,
}

impl Sink {
    /// Creates a new sink.
    pub fn new(
        input_sample_rate: SampleRate,
        input_channels: ChannelCount,
        output_sample_rate: SampleRate,
        output_channels: ChannelCount,
        output_buffer: Arc<Mutex<BoxAudioBuffer>>,
        broadcaster: Broadcaster<AudioDeviceMessage>,
    ) -> Self {
        let (chunk_size_frames, resampler) = if input_sample_rate != output_sample_rate {
            let resampler = FftFixedInOut::new(
                input_sample_rate as usize,
                output_sample_rate as usize,
                DESIRED_CHUNK_SIZE_FRAMES,
                input_channels as usize,
            )
            .expect("failed to create resampler (this is a bug)");
            (resampler.input_frames_max(), Some(RefCell::new(resampler)))
        } else {
            (DESIRED_CHUNK_SIZE_FRAMES, None)
        };
        let subscription = broadcaster.subscribe("audio-sink", AudioDeviceMessageChannel::Requests);
        Self {
            input_sample_rate,
            input_channels,
            output_sample_rate,
            output_channels,
            desired_input_frames: (DESIRED_QUEUE_LENGTH.as_secs_f32() * input_sample_rate as f32)
                as usize,
            chunk_size_frames,
            resampler,
            resample_buffers: RefCell::new(ResampleBuffers {
                input: SourceBuffer::empty(0, 0),
                output: SourceBuffer::empty(0, 0),
            }),
            input_buffer: Arc::new(Mutex::new(SourceBuffer::empty(
                input_sample_rate,
                input_channels,
            ))),
            output_buffer,
            subscription,
        }
    }

    /// The expected sample rate of the input.
    pub fn input_sample_rate(&self) -> SampleRate {
        self.input_sample_rate
    }

    /// The expected number of channels in the input.
    pub fn input_channels(&self) -> ChannelCount {
        self.input_channels
    }

    /// True if more audio data is needed to feed the audio device.
    pub fn needs_more_chunks(&self) -> bool {
        self.input_buffer.lock().unwrap().frame_count() < self.desired_input_frames
    }

    fn remix_and_resample_to_output(
        &self,
        original: &mut SourceBuffer,
        final_output: &mut BoxAudioBuffer,
    ) {
        let mut resample_buffers = self.resample_buffers.borrow_mut();
        let ResampleBuffers {
            ref mut input,
            ref mut output,
        } = &mut *resample_buffers;

        let resampler_borrow = self.resampler.as_ref().map(|r| r.borrow_mut());

        input.make_empty_with_channels(original.channel_count());
        original.drain_into(self.chunk_size_frames, input);

        input.remix_in_place(self.output_channels);
        let mut final_buffer = &input;
        if let Some(mut resampler) = resampler_borrow {
            input.resample_into(output, self.output_sample_rate, &mut *resampler);
            final_buffer = &output;
        }

        final_output.extend(final_buffer);
    }

    /// This is a blocking call that sends data to the audio device as needed.
    pub fn send_audio_with_timeout(&self, timeout: Duration) {
        if let Some(AudioDeviceMessage::RequestAudioData) = self.subscription.recv_timeout(timeout)
        {
            let mut input_buffer = self.input_buffer.lock().unwrap();
            if input_buffer.frame_count() >= self.chunk_size_frames {
                let mut output_buffer = self.output_buffer.lock().unwrap();
                self.remix_and_resample_to_output(&mut input_buffer, &mut output_buffer);
            }
        }
    }

    /// Queues more audio data to be sent to the audio device.
    ///
    /// # Panics
    ///
    /// This panics if the source sample rate or channel count doesn't match
    /// the expected sample rate or channel count.
    pub fn queue(&self, source: &SourceBuffer) {
        // The sink needs to be recreated if the sample rate or number of channels changes
        debug_assert!(source.sample_rate() == self.input_sample_rate);
        debug_assert!(source.channel_count() == self.input_channels);

        let mut input_buffer = self.input_buffer.lock().unwrap();
        input_buffer.extend(source);
    }

    /// Flushes any remaining audio data to the audio device.
    pub fn flush(&self) {
        let mut input_buffer = self.input_buffer.lock().unwrap();
        if input_buffer.frame_count() == 0 {
            return;
        }
        if input_buffer.frame_count() < self.chunk_size_frames {
            input_buffer.extend_with_silence(self.chunk_size_frames);
        }

        let mut output_buffer = self.output_buffer.lock().unwrap();
        self.remix_and_resample_to_output(&mut input_buffer, &mut output_buffer);
    }
}

/// A typed audio buffer.
pub struct AudioBuffer<S> {
    data: Vec<S>,
}

impl<S> AudioBuffer<S> {
    /// Creates a new audio buffer.
    pub fn new(data: Vec<S>) -> Self {
        Self { data }
    }

    /// Clears the buffer.
    pub fn clear(&mut self) {
        self.data.clear();
    }

    /// Drains the given range from the buffer and returns it as an iterator.
    ///
    /// Behaves the same as [`Vec::drain`] in the standard library.
    pub fn drain<R>(&mut self, range: R) -> impl Iterator<Item = S> + '_
    where
        R: RangeBounds<usize>,
    {
        self.data.drain(range)
    }

    /// The length of this buffer.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether or not this buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl<S> AudioBuffer<S>
where
    S: symphonia::core::sample::Sample,
    S: symphonia::core::conv::FromSample<f32>,
{
    /// Extend this buffer with data from a source buffer.
    fn extend(&mut self, source: &SourceBuffer) {
        debug_assert!(source.channel_count() > 0);

        source.extend_interleaved_into(&mut self.data);
    }
}

/// A boxed audio buffer.
///
/// This is used to erase the underlying sample type
/// to stop the virality of generics.
pub struct BoxAudioBuffer {
    format: SampleFormat,
    inner_format: &'static str,
    inner: Box<dyn Any + Send>,
}

impl BoxAudioBuffer {
    /// Creates a new boxed audio buffer.
    pub fn new<S: Sample + Send + 'static>(format: SampleFormat, buffer: AudioBuffer<S>) -> Self {
        Self {
            format,
            inner_format: std::any::type_name::<S>(),
            inner: Box::new(buffer),
        }
    }

    /// Creates an empty boxed audio buffer of the given format.
    pub fn empty(format: SampleFormat) -> Self {
        Self {
            format,
            inner_format: match format {
                SampleFormat::F32 => "f32",
                SampleFormat::F64 => "f64",
                SampleFormat::I16 => "i16",
                SampleFormat::I32 => "i32",
                SampleFormat::I8 => "i8",
                SampleFormat::U16 => "u16",
                SampleFormat::U32 => "u32",
                SampleFormat::U8 => "u8",
                SampleFormat::I64 | SampleFormat::U64 => {
                    unreachable!("unsupported: {}", format)
                }
                _ => unreachable!("{}", format),
            },
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

    /// Extends this buffer with the given source buffer.
    ///
    /// __Important:__ the source buffer _must_ be the same sample rate and
    /// channel count as this buffer. If it is not, there will be corruption
    /// of the audio data rather than a panic or error since the audio buffer
    /// is not aware of its own sample rate and channel count.
    pub fn extend(&mut self, source: &SourceBuffer) {
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

    /// Clears this buffer.
    pub fn clear(&mut self) {
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

    /// Returns a mutable reference to the underlying typed audio buffer.
    #[inline]
    pub fn get_mut<S: Sample + 'static>(&mut self) -> Option<&mut AudioBuffer<S>> {
        self.inner.downcast_mut::<AudioBuffer<S>>()
    }
    /// Returns a mutable reference to the underlying typed audio buffer.
    ///
    /// Panics if the underlying type is not the expected type.
    #[inline]
    pub fn expect_mut<S: Sample + 'static>(&mut self) -> &mut AudioBuffer<S> {
        let inner_format = self.inner_format;
        self.get_mut()
            .ok_or_else(|| {
                format!(
                    "failed to downcast {} audio buffer to {}",
                    inner_format,
                    std::any::type_name::<S>()
                )
            })
            .unwrap()
    }
}
