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

use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt;

use millenium_core::player::PlayerThreadError;
use millenium_desktop_assets::AssetError;

/// A boxed error that is send and sync.
pub type BoxError = Box<dyn StdError + Send + Sync>;

/// A fatal error that should be reported to the user.
#[derive(Debug)]
pub struct FatalError {
    message: Option<Cow<'static, str>>,
    source: Option<BoxError>,
}

#[allow(dead_code)]
impl FatalError {
    /// Creates a new fatal error with a source.
    pub fn new(what: impl Into<Cow<'static, str>>, source: impl Into<BoxError>) -> Self {
        Self {
            message: Some(what.into()),
            source: Some(source.into()),
        }
    }

    /// Creates a new fatal error from a message.
    pub fn msg(what: impl Into<Cow<'static, str>>) -> Self {
        Self {
            message: Some(what.into()),
            source: None,
        }
    }

    /// Creates a new fatal error with a source only.
    pub fn source_only(source: impl Into<BoxError>) -> Self {
        Self {
            message: None,
            source: Some(source.into()),
        }
    }
}

impl StdError for FatalError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as _)
    }
}

impl fmt::Display for FatalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(message) = &self.message {
            write!(f, "{}", message)?;
        }
        if let Some(source) = &self.source {
            write!(
                f,
                "{}{}",
                if self.message.is_some() { ": " } else { "" },
                source
            )?;
        }
        Ok(())
    }
}

impl From<clap::Error> for FatalError {
    fn from(err: clap::Error) -> Self {
        Self::source_only(err)
    }
}

impl From<PlayerThreadError> for FatalError {
    fn from(err: PlayerThreadError) -> Self {
        Self::source_only(err)
    }
}

impl From<AssetError> for FatalError {
    fn from(err: AssetError) -> Self {
        Self::source_only(err)
    }
}
