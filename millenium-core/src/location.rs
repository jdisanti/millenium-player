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
use std::{error::Error as StdError, str::FromStr};
use thiserror::Error;
use url::Url;

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
}
