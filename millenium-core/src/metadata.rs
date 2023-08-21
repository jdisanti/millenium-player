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

use std::{borrow::Cow, cmp::Ordering, collections::BTreeSet, fmt, sync::Arc};
use symphonia_core::meta::{StandardTagKey, StandardVisualKey};

#[derive(Debug, thiserror::Error)]
#[error("{}", self.0)]
pub struct MetadataConversionError(&'static str);

#[derive(Clone, Debug, Default)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct Metadata {
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub artist: Option<String>,
    pub composer: Option<String>,
    pub cover: Option<EmbeddedImage>,
    pub genre: Option<String>,
    pub track_number: Option<String>,
    pub track_total: Option<String>,
    pub track_title: Option<String>,
    pub other: BTreeSet<Tag>,
}

impl TryFrom<&symphonia_core::meta::Metadata<'_>> for Metadata {
    type Error = MetadataConversionError;

    fn try_from(value: &symphonia_core::meta::Metadata<'_>) -> Result<Self, Self::Error> {
        let latest = value
            .current()
            .ok_or(MetadataConversionError("failed to read metadata in source"))?;
        let mut meta = Metadata::default();
        for stag in latest.tags() {
            let tag = Tag::from(stag);
            match stag.std_key {
                Some(StandardTagKey::Album) => {
                    meta.album = Some(tag.value.into());
                }
                Some(StandardTagKey::AlbumArtist) => {
                    meta.album_artist = Some(tag.value.into());
                }
                Some(StandardTagKey::Artist) => {
                    meta.artist = Some(tag.value.into());
                }
                Some(StandardTagKey::Composer) => {
                    meta.composer = Some(tag.value.into());
                }
                Some(StandardTagKey::Genre) => {
                    meta.genre = Some(tag.value.into());
                }
                Some(StandardTagKey::TrackNumber) => {
                    meta.track_number = Some(tag.value.into());
                }
                Some(StandardTagKey::TrackTotal) => {
                    meta.track_total = Some(tag.value.into());
                }
                Some(StandardTagKey::TrackTitle) => {
                    meta.track_title = Some(tag.value.into());
                }
                _ => {
                    meta.other.insert(tag);
                }
            }
        }
        for visual in latest.visuals() {
            if let Some(StandardVisualKey::FrontCover) = visual.usage {
                meta.cover = Some(visual.into());
            }
        }
        Ok(meta)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tag {
    pub key: String,
    pub value: Cow<'static, str>,
}

impl PartialOrd for Tag {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match self.key.partial_cmp(&other.key) {
            Some(Ordering::Equal) => {}
            ord => return ord,
        }
        self.value.partial_cmp(&other.value)
    }
}

impl Ord for Tag {
    fn cmp(&self, other: &Self) -> Ordering {
        PartialOrd::partial_cmp(self, other).unwrap()
    }
}

impl From<&symphonia_core::meta::Tag> for Tag {
    fn from(value: &symphonia_core::meta::Tag) -> Self {
        use base64::engine::{general_purpose::STANDARD as b64, Engine as _};
        use symphonia_core::meta::Value;
        Tag {
            key: value.key.clone(),
            value: match &value.value {
                Value::Binary(bin) => Cow::Owned(b64.encode(bin)),
                Value::Boolean(b) => Cow::Borrowed(if *b { "true" } else { "false" }),
                Value::Flag => Cow::Borrowed(""),
                Value::Float(f) => Cow::Owned(f.to_string()),
                Value::SignedInt(i) => Cow::Owned(i.to_string()),
                Value::String(s) => Cow::Owned(s.clone()),
                Value::UnsignedInt(u) => Cow::Owned(u.to_string()),
            },
        }
    }
}

#[derive(Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct EmbeddedImage {
    pub mime_type: String,
    pub tags: BTreeSet<Tag>,
    pub data: Arc<Vec<u8>>,
}

impl fmt::Debug for EmbeddedImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddedImage")
            .field("mime_type", &self.mime_type)
            .field("tags", &self.tags)
            .field("data", &format!("** snipped {} bytes **", self.data.len()))
            .finish()
    }
}

impl From<&symphonia_core::meta::Visual> for EmbeddedImage {
    fn from(value: &symphonia_core::meta::Visual) -> Self {
        Self {
            mime_type: value.media_type.clone(),
            tags: value.tags.iter().map(Tag::from).collect::<BTreeSet<_>>(),
            data: Arc::new(value.data.to_vec()),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;
    use symphonia_core::{
        formats::FormatOptions,
        io::{MediaSourceStream, MediaSourceStreamOptions},
        meta::MetadataOptions,
        probe::Hint,
    };

    #[test]
    fn metadata_conversion() {
        let file = File::open("../test-data/hydrate/hydrate.mp3").unwrap();
        let stream = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
        let probe = symphonia::default::get_probe();
        let mut format = probe
            .format(
                Hint::new().with_extension("mp3"),
                stream,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .unwrap();
        let mut meta = format.metadata.get().unwrap();
        meta.skip_to_latest();

        let mut meta = Metadata::try_from(&meta).unwrap();
        let cover = meta.cover.take().unwrap();

        pretty_assertions::assert_eq!(
            Metadata {
                album: None,
                album_artist: None,
                artist: Some("kenny beltrey".into()),
                composer: None,
                cover: None,
                genre: Some("Electronic".into()),
                track_number: None,
                track_total: None,
                track_title: Some("hydrate (the beach)".into()),
                other: [("COMM!eng", "kahvi #011 - kahvi.stc.cx"), ("TYER", "2000")]
                    .iter()
                    .map(|&(k, v)| Tag {
                        key: k.into(),
                        value: v.into(),
                    })
                    .collect(),
            },
            meta,
        );

        assert_eq!("image/jpeg", cover.mime_type);
        assert_eq!(226833, cover.data.len());
    }
}
