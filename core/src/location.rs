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

use camino::{Utf8Path, Utf8PathBuf};
use std::{error::Error as StdError, fmt, str::FromStr};
use thiserror::Error;
use url::Url;

#[derive(Copy, Clone, Debug)]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub enum InferredLocationType {
    Audio,
    Playlist,
    Unknown,
}

impl InferredLocationType {
    /// True if the inferred type is a playlist.
    pub fn is_playlist(&self) -> bool {
        matches!(self, Self::Playlist { .. })
    }

    /// True if the inferred type is unknown.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}

/// Resource location that can either be a URL or file path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Location {
    Url(Url),
    Path(Utf8PathBuf),
}

impl Location {
    /// Create a new location from a URL.
    pub fn url(url: impl Into<Url>) -> Self {
        Self::Url(url.into())
    }

    /// Create a new location from a file system path.
    pub fn path(path: impl Into<Utf8PathBuf>) -> Self {
        Self::Path(path.into())
    }

    /// Returns this location as a URL, if it is one.
    pub fn as_url(&self) -> Option<&Url> {
        match self {
            Self::Url(url) => Some(url),
            Self::Path(_) => None,
        }
    }

    /// Returns this location as a file system path, if it is one.
    pub fn as_path(&self) -> Option<&Utf8Path> {
        match self {
            Self::Url(_) => None,
            Self::Path(path) => Some(path),
        }
    }

    /// Returns this location as a string.
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }

    /// Returns the file extension on this location, if there is one.
    pub fn extension(&self) -> Option<&str> {
        match self {
            Self::Url(url) => Utf8Path::new(url.path()).extension(),
            Self::Path(path) => path.extension(),
        }
    }

    /// Infers the type of the location.
    pub fn inferred_type(&self) -> InferredLocationType {
        let lower_ext: Option<String> = match self {
            Self::Url(url) => Utf8Path::new(&url.path())
                .extension()
                .map(str::to_ascii_lowercase),
            Self::Path(path) => path.extension().map(|ext| ext.to_ascii_lowercase()),
        };
        if let Some(lower_ext) = lower_ext {
            match lower_ext.as_str() {
                "m3u" | "m3u8" | "pls" => InferredLocationType::Playlist,
                "aac" => InferredLocationType::Audio,
                "mp1" | "mp2" | "mp3" | "mp4" | "m4a" => InferredLocationType::Audio,
                "ogg" | "oga" | "opus" | "flac" => InferredLocationType::Audio,
                "wav" => InferredLocationType::Audio,
                "webm" => InferredLocationType::Audio,
                _ => InferredLocationType::Unknown,
            }
        } else {
            InferredLocationType::Unknown
        }
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Location {
    type Err = ParseLocationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains("://") {
            Ok(Self::Url(Url::parse(s).map_err(|source| {
                ParseLocationError {
                    location: s.to_owned(),
                    source: Box::new(source),
                }
            })?))
        } else {
            Ok(Self::Path(Utf8PathBuf::from_str(s).map_err(|source| {
                ParseLocationError {
                    location: s.to_owned(),
                    source: Box::new(source),
                }
            })?))
        }
    }
}

impl AsRef<str> for Location {
    fn as_ref(&self) -> &str {
        match self {
            Self::Path(path) => path.as_str(),
            Self::Url(url) => url.as_str(),
        }
    }
}

impl<'de> serde::Deserialize<'de> for Location {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, Unexpected};
        let s = String::deserialize(deserializer)?;
        Location::from_str(&s)
            .map_err(|_| Error::invalid_value(Unexpected::Str(&s), &"valid location"))
    }
}

impl serde::Serialize for Location {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Debug, Error)]
#[error("failed to parse location \"{location}\": {source}")]
pub struct ParseLocationError {
    location: String,
    source: Box<dyn StdError>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_location() {
        pretty_assertions::assert_eq!(
            Location::path("foo"),
            Location::from_str("foo").expect("success")
        );
        pretty_assertions::assert_eq!(
            Location::path("foo.mp3"),
            Location::from_str("foo.mp3").expect("success")
        );
        pretty_assertions::assert_eq!(
            Location::path("path/to/foo.mp3"),
            Location::from_str("path/to/foo.mp3").expect("success")
        );

        pretty_assertions::assert_eq!(
            Location::url(Url::parse("https://example.com/foo").expect("success")),
            Location::from_str("https://example.com/foo").expect("success")
        );
        pretty_assertions::assert_eq!(
            Location::url(Url::parse("https://example.com/foo.mp3").expect("success")),
            Location::from_str("https://example.com/foo.mp3").expect("success")
        );

        let err = Location::from_str("://example.com/foo.mp3").expect_err("should fail");
        let err = err.to_string();
        assert_eq!(
            "failed to parse location \"://example.com/foo.mp3\": relative URL without a base",
            err
        );
    }

    #[test]
    fn infer_type() {
        let playlist_extensions = &[".m3u", ".m3u8", ".pls"];
        let audio_extensions = &[
            ".aac", ".mp1", ".mp2", ".mp3", ".mp4", ".m4a", ".ogg", ".oga", ".opus", ".flac",
            ".wav", ".webm",
        ];
        for ext in playlist_extensions {
            assert_eq!(
                InferredLocationType::Playlist,
                Location::path(format!("foo{}", ext)).inferred_type()
            );
            assert_eq!(
                InferredLocationType::Playlist,
                Location::from_str(&format!("https://example.com/foo{}", ext))
                    .unwrap()
                    .inferred_type()
            );
        }
        for ext in audio_extensions {
            assert_eq!(
                InferredLocationType::Audio,
                Location::path(format!("foo{}", ext)).inferred_type()
            );
            assert_eq!(
                InferredLocationType::Audio,
                Location::from_str(&format!("https://example.com/foo{}", ext))
                    .unwrap()
                    .inferred_type()
            );
        }
        assert_eq!(
            InferredLocationType::Unknown,
            Location::path("foo").inferred_type()
        );
        assert_eq!(
            InferredLocationType::Unknown,
            Location::path("foo.asdf").inferred_type()
        );
        assert_eq!(
            InferredLocationType::Unknown,
            Location::path("https://example.com/foo").inferred_type()
        );
    }

    #[test]
    fn extension() {
        assert_eq!(None, Location::path("test").extension());
        assert_eq!(Some("foo"), Location::path("test.foo").extension());
        assert_eq!(
            Some("foo"),
            Location::from_str("https://example.com/test.foo")
                .unwrap()
                .extension()
        );
    }

    #[test]
    fn serde() {
        assert_eq!(
            "\"https://example.com/\"",
            serde_json::to_string(&Location::from_str("https://example.com/").unwrap()).unwrap()
        );
        assert_eq!(
            "\"/path/to/something\"",
            serde_json::to_string(&Location::from_str("/path/to/something").unwrap()).unwrap()
        );
        assert_eq!(
            Location::from_str("https://example.com/").unwrap(),
            serde_json::from_str("\"https://example.com/\"").unwrap(),
        );
        assert_eq!(
            Location::path("/path/to/something"),
            serde_json::from_str("\"/path/to/something\"").unwrap(),
        );
    }
}
