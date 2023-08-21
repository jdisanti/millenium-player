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
    location::Location,
    metadata::{Metadata, MetadataConversionError},
};
use camino::Utf8PathBuf;
use std::error::Error as StdError;
use std::fs::File;
use symphonia_core::{
    audio::AudioBufferRef,
    codecs::{Decoder, DecoderOptions},
    formats::FormatReader,
    io::MediaSourceStream,
    probe::Hint,
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

pub(super) struct AudioSource {
    _location: Location,
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    metadata: Option<Metadata>,
}

impl AudioSource {
    pub(super) fn new(location: Location) -> Result<Self, AudioSourceError> {
        let Stream {
            reader,
            decoder,
            metadata,
        } = load_stream(&location, None)?;
        Ok(Self {
            _location: location,
            reader,
            decoder,
            metadata,
        })
    }

    pub(super) fn metadata(&self) -> Option<&Metadata> {
        self.metadata.as_ref()
    }

    pub(super) fn next_chunk(&mut self) -> Result<Option<AudioBufferRef<'_>>, AudioSourceError> {
        let packet = match self.reader.next_packet() {
            Ok(packet) => packet,
            // Symphonia's end of stream is an IO error with unexpected EOF
            Err(symphonia_core::errors::Error::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                return Ok(None)
            }
            Err(err) => return Err(AudioSourceError::FailedToReadStream { source: err.into() }),
        };
        self.decoder
            .decode(&packet)
            .map(Some)
            .map_err(|err| AudioSourceError::FailedToDecodeStream { source: err.into() })
    }
}

struct Stream {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    metadata: Option<Metadata>,
}

fn load_stream(
    location: &Location,
    existing_metadata: Option<Metadata>,
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
            .map(|mut meta| {
                meta.skip_to_latest();
                Metadata::try_from(&meta)
            })
            .transpose()?
    };

    let codecs = symphonia::default::get_codecs();
    let track = format
        .format
        .default_track()
        .ok_or(AudioSourceError::SourceHadNoAudioTracks)?;

    let decoder = codecs
        .make(&track.codec_params, &DecoderOptions { verify: true })
        .map_err(|err| AudioSourceError::FailedToCreateAudioDecoder { source: err.into() })?;

    Ok(Stream {
        reader: format.format,
        decoder,
        metadata,
    })
}
