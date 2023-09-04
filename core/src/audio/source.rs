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

use crate::{
    audio::{ChannelCount, SampleRate},
    location::Location,
    metadata::{Metadata, MetadataConversionError},
};
use camino::Utf8PathBuf;
use rubato::ResampleResult;
use std::fs::File;
use std::{cmp::Ordering, error::Error as StdError};
use symphonia::core::{
    audio::{AudioBuffer, AudioBufferRef, Signal},
    codecs::{Decoder, DecoderOptions},
    conv::{FromSample, IntoSample},
    formats::{FormatReader, Track},
    io::MediaSourceStream,
    probe::Hint,
    sample::Sample,
};

#[derive(Debug, thiserror::Error)]
pub enum AudioSourceError {
    #[error("failed to load audio stream: {source}")]
    FailedToLoadStream {
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },
    #[error("failed to load file \"{path}\": {source}")]
    FailedToLoadFile {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read metadata: {source}")]
    FailedToReadMetadata {
        #[source]
        #[from]
        source: MetadataConversionError,
    },
    #[error("failed to read audio stream: {source}")]
    FailedToReadStream {
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },
    #[error("failed to decode audio stream: {source}")]
    FailedToDecodeStream {
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },
    #[error("source contained no audio tracks")]
    SourceHadNoAudioTracks,
    #[error("failed to create audio decoder: {source}")]
    FailedToCreateAudioDecoder {
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },
}

/// Specialized object-safe adapter for Rubato's [`Resampler`](rubato::Resampler) trait.
pub trait Resampler {
    /// Resample the given channels into a new set of channels.
    ///
    /// The destination frequency is determined by the resampler's configuration.
    fn resample(&mut self, channels: &[Vec<f32>]) -> ResampleResult<Vec<Vec<f32>>>;
}

impl<R> Resampler for R
where
    R: rubato::Resampler<f32>,
{
    fn resample(&mut self, channels: &[Vec<f32>]) -> ResampleResult<Vec<Vec<f32>>> {
        self.process(channels, None)
    }
}

/// A buffer of audio data that always has samples represented as 32-bit floats,
/// and channels that are not interleaved.
///
/// This buffer sits between the audio decoder and the audio sink to provide
/// a consistent format for audio transformations such as resampling, remixing,
/// and volume adjustment.
#[derive(Clone, Debug)]
pub struct SourceBuffer {
    sample_rate: SampleRate,
    channels: Vec<Vec<f32>>,
}
impl SourceBuffer {
    /// Creates an empty source buffer.
    pub fn empty(sample_rate: SampleRate, channels: ChannelCount) -> Self {
        Self {
            sample_rate,
            channels: vec![Vec::new(); channels as usize],
        }
    }

    /// Clears this buffer.
    pub fn clear(&mut self) {
        for channel in &mut self.channels {
            channel.clear();
        }
    }

    /// Extend this buffer with another buffer's data.
    pub fn extend(&mut self, other: SourceBuffer) {
        debug_assert!(other.sample_rate() == self.sample_rate);
        debug_assert!(other.channel_count() == self.channel_count());
        for (into, from) in self.channels.iter_mut().zip(other.channels.iter()) {
            into.extend(from.iter());
        }
    }

    /// Extend this buffer to the given frame count with silence.
    pub fn extend_with_silence(&mut self, desired_frames: usize) {
        debug_assert!(self.frame_count() < desired_frames);
        for channel in &mut self.channels {
            channel.resize(desired_frames, 0.0);
        }
    }

    /// Drain the first N frames from the buffer and return them as a new buffer.
    pub fn drain(&mut self, n: usize) -> SourceBuffer {
        debug_assert!(self.frame_count() >= n);
        Self {
            sample_rate: self.sample_rate,
            channels: self
                .channels
                .iter_mut()
                .map(|channel| channel.drain(0..n).collect())
                .collect(),
        }
    }

    /// The sample rate of the source buffer.
    pub fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    /// The number of frames currently in the source buffer.
    pub fn frame_count(&self) -> usize {
        self.channels.get(0).map(Vec::len).unwrap_or(0)
    }

    /// The number of channels in the source buffer.
    pub fn channel_count(&self) -> ChannelCount {
        self.channels.len() as ChannelCount
    }

    /// Raw samples for the given channel.
    ///
    /// Panics if the channel index is out of bounds.
    pub fn channel(&self, channel: usize) -> &[f32] {
        self.channels[channel].as_slice()
    }

    /// Resamples this buffer into a new buffer with the given resampler.
    pub fn resampled(&self, new_sample_rate: SampleRate, resampler: &mut dyn Resampler) -> Self {
        Self {
            sample_rate: new_sample_rate,
            channels: resampler
                .resample(&self.channels)
                .expect("failed to resample (this is a bug)"),
        }
    }

    /// Remixes into a different arrangement of channels in place.
    pub fn remix(self, new_channels: ChannelCount) -> Self {
        match self.channel_count().cmp(&new_channels) {
            Ordering::Equal => self,
            Ordering::Less => self.up_mix(new_channels),
            Ordering::Greater => self.down_mix(new_channels),
        }
    }

    fn up_mix(self, new_channels: ChannelCount) -> Self {
        // Mono to stereo
        if self.channel_count() == 1 && new_channels == 2 {
            // 10^(dB/20) with dB=-3
            let gain = 0.707_945_76;
            let mut attenuated = self.channels.into_iter().next().unwrap();
            attenuated.iter_mut().for_each(|s| *s *= gain);
            return Self {
                sample_rate: self.sample_rate,
                channels: vec![attenuated.clone(), attenuated],
            };
        }

        unimplemented!(
            "up mixing from {} to {} channels isn't implemented yet",
            self.channel_count(),
            new_channels
        )
    }

    fn down_mix(mut self, new_channels: ChannelCount) -> Self {
        // Stereo to mono
        if self.channel_count() == 2 && new_channels == 1 {
            // 10^(dB/20) with dB=3
            let gain = 1.412_537_6;
            let mut left = self.channels.remove(0);
            let right = self.channels.remove(0);
            left.iter_mut()
                .zip(right.iter())
                .for_each(|(left, right)| *left = (*left * gain + right * gain).clamped());
            return Self {
                sample_rate: self.sample_rate,
                channels: vec![left],
            };
        }

        unimplemented!(
            "down mixing from {} to {} channels isn't implemented yet",
            self.channel_count(),
            new_channels
        )
    }

    /// Interleave into the given vec in the required sample format.
    ///
    /// This extends the given vec rather than overwrite it.
    pub fn extend_interleaved_into<Format>(&self, into: &mut Vec<Format>)
    where
        Format: Sample,
        Format: FromSample<f32>,
    {
        let frame_count = self.frame_count();
        let interleaved_len: usize = frame_count * self.channel_count() as usize;
        let start = into.len();
        into.resize(into.len() + interleaved_len, Format::MID);

        for i in 0..(self.channel_count() as usize) {
            let mut into_iter = into
                .iter_mut()
                .skip(start + i)
                .step_by(self.channel_count() as usize);
            for &sample in self.channel(i) {
                *into_iter.next().unwrap() = Format::from_sample(sample);
            }
        }
    }

    // TODO: Do this conversion in place to reduce allocations
    fn from_symphonia(from: AudioBufferRef) -> Self {
        fn convert_and_copy<S>(channel: usize, from: &AudioBuffer<S>, into: &mut Vec<f32>)
        where
            S: Sample,
            S: IntoSample<f32>,
        {
            let from_iter = from.chan(channel).iter().map(|&s| s.into_sample());
            let to_iter = into.as_mut_slice().iter_mut();
            for (to, from) in to_iter.zip(from_iter) {
                *to = from;
            }
        }
        macro_rules! convert_and_copy {
            ($channel:ident, $from:expr => $into:expr, $($format:ident,)+) => {
                match $from {
                    $(AudioBufferRef::$format(b) => convert_and_copy($channel, b, $into),)+
                }
            };
        }

        let sample_rate = from.spec().rate;
        let channel_count = from.spec().channels.count();
        let frame_count = from.frames();
        let mut channels = Vec::with_capacity(channel_count);
        for channel in 0..channel_count {
            let mut data = vec![0.0; frame_count];
            convert_and_copy!(
                channel,
                &from => &mut data,
                U8, U16, U24, U32, S8, S16, S24, S32, F32, F64,
            );
            channels.push(data);
        }
        Self {
            sample_rate,
            channels,
        }
    }
}

/// Preferred format to use when decoding audio.
///
/// This is used to decide which track to select in a multi-track file.
#[derive(Copy, Clone)]
pub struct PreferredFormat {
    pub sample_rate: SampleRate,
    pub channel_count: ChannelCount,
}

impl PreferredFormat {
    pub fn new(sample_rate: SampleRate, channel_count: ChannelCount) -> Self {
        Self {
            sample_rate,
            channel_count,
        }
    }
}

/// An audio decoder source.
pub struct AudioDecoderSource {
    _location: Location,
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    metadata: Option<Metadata>,
    frame_count: Option<u64>,
    selected_track_id: u32,
}

impl AudioDecoderSource {
    /// Creates a new audio decoder source with the given location.
    ///
    /// This will load the stream and metadata synchronously.
    pub fn new(
        location: Location,
        preferred_format: PreferredFormat,
    ) -> Result<Self, AudioSourceError> {
        let Stream {
            reader,
            decoder,
            metadata,
            frame_count,
            selected_track_id,
        } = load_stream(&location, None, preferred_format)?;
        Ok(Self {
            _location: location,
            reader,
            decoder,
            metadata,
            frame_count,
            selected_track_id,
        })
    }

    /// The metadata from the tags on this source.
    pub fn metadata(&self) -> Option<&Metadata> {
        self.metadata.as_ref()
    }

    /// The number of frames this stream contains, if available.
    pub fn frame_count(&self) -> Option<u64> {
        self.frame_count
    }

    /// Retrieve and decode the next chunk of audio data.
    ///
    /// Returns `Ok(None)` if the stream has ended.
    pub fn next_chunk(&mut self) -> Result<Option<SourceBuffer>, AudioSourceError> {
        let packet = loop {
            match self.reader.next_packet() {
                Ok(packet) => {
                    if packet.track_id() == self.selected_track_id {
                        break packet;
                    }
                }
                // Symphonia's end of stream is an IO error with unexpected EOF
                Err(symphonia::core::errors::Error::IoError(err))
                    if err.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None)
                }
                Err(err) => {
                    return Err(AudioSourceError::FailedToReadStream { source: err.into() })
                }
            };
        };
        self.decoder
            .decode(&packet)
            .map(SourceBuffer::from_symphonia)
            .map(Some)
            .map_err(|err| AudioSourceError::FailedToDecodeStream { source: err.into() })
    }
}

struct Stream {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    metadata: Option<Metadata>,
    frame_count: Option<u64>,
    selected_track_id: u32,
}

fn load_stream(
    location: &Location,
    existing_metadata: Option<Metadata>,
    preferred_format: PreferredFormat,
) -> Result<Stream, AudioSourceError> {
    let media_stream = match location {
        Location::Url(_url) => {
            unimplemented!("streaming from URLs is not yet supported")
        }
        Location::Path(path) => MediaSourceStream::new(
            Box::new(
                File::open(path).map_err(|err| AudioSourceError::FailedToLoadFile {
                    path: path.clone(),
                    source: err,
                })?,
            ),
            Default::default(),
        ),
    };
    let probe = symphonia::default::get_probe();
    let mut hint = Hint::new();
    // TODO: Add mime hint for streaming URLs
    if let Some(extension) = location.extension() {
        hint.with_extension(extension);
    }

    let mut format = probe
        .format(
            &hint,
            media_stream,
            &Default::default(),
            &Default::default(),
        )
        .map_err(|err| AudioSourceError::FailedToLoadStream {
            source: Box::new(err),
        })?;
    let metadata = if let Some(existing_metadata) = existing_metadata {
        Some(existing_metadata)
    } else {
        format
            .metadata
            .get()
            .or_else(|| Some(format.format.metadata()))
            .map(|mut meta| {
                meta.skip_to_latest();
                Metadata::try_from(&meta)
            })
            .transpose()?
    };

    let codecs = symphonia::default::get_codecs();
    let selected_track = select_track(&*format.format, preferred_format)?;
    let selected_track_id = selected_track.id;
    let frame_count = selected_track.codec_params.n_frames;

    let decoder = codecs
        .make(
            &selected_track.codec_params,
            &DecoderOptions { verify: true },
        )
        .map_err(|err| AudioSourceError::FailedToCreateAudioDecoder { source: err.into() })?;

    Ok(Stream {
        reader: format.format,
        decoder,
        metadata,
        frame_count,
        selected_track_id,
    })
}

fn select_track(
    format_reader: &dyn FormatReader,
    preferred_format: PreferredFormat,
) -> Result<&Track, AudioSourceError> {
    let tracks = format_reader.tracks();
    if tracks.is_empty() {
        Err(AudioSourceError::SourceHadNoAudioTracks)
    } else if tracks.len() == 1 {
        Ok(&tracks[0])
    } else {
        log::info!("multiple audio tracks found:");
        let (mut preferred_by_channels, mut preferred_by_samples) = (None, None);
        for track in tracks {
            let channels = track
                .codec_params
                .channels
                .map(|c| c.count())
                .unwrap_or_default() as ChannelCount;
            if channels == preferred_format.channel_count {
                preferred_by_channels = Some(track);
            }
            let sample_rate = track.codec_params.sample_rate.unwrap_or_default();
            if sample_rate == preferred_format.sample_rate {
                preferred_by_samples = Some(track);
            }
            log::info!(
                "  track {id}: {channels} channels, {sample_rate} Hz",
                id = track.id
            );
        }
        let selected_track = preferred_by_channels
            .or(preferred_by_samples)
            .or(Some(&tracks[0]))
            .expect("there is at least one track");
        log::info!(
            "selected track {id} because {reason}",
            id = selected_track.id,
            reason = match (preferred_by_channels, preferred_by_samples) {
                (Some(_), _) => "it has the right number of channels for the audio output device",
                (None, Some(_)) => "its sample format matches the audio output device",
                _ => "there was no track that matched the audio output device format",
            }
        );
        Ok(selected_track)
    }
}
