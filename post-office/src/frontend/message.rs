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

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize))]
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-util"), derive(PartialEq))]
#[serde(tag = "kind")]
pub enum FrontendMessage {
    DragWindowStart,
    LoadLocations {
        locations: Vec<String>,
    },
    Log {
        level: LogLevel,
        message: String,
    },
    MediaControlBack,
    MediaControlForward,
    MediaControlPause,
    MediaControlPlay,
    MediaControlSeek {
        position: usize,
    },
    MediaControlSkipBack,
    MediaControlSkipForward,
    MediaControlStop,
    MediaControlPlaylistMode {
        mode: PlaylistMode,
    },
    Quit,
    ShowAlert {
        level: AlertLevel,
        message: Cow<'static, str>,
    },
    PlaybackStateUpdated,
    WaveformStateUpdated,
}

#[cfg(feature = "broadcast")]
impl crate::broadcast::BroadcastMessage for FrontendMessage {
    type Channel = crate::broadcast::NoChannels;

    fn channel(&self) -> Self::Channel {
        crate::broadcast::NoChannels
    }

    fn frequent(&self) -> bool {
        matches!(self, Self::Log { .. })
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize))]
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
pub enum PlaylistMode {
    #[default]
    Normal,
    RepeatOne,
    RepeatAll,
    Shuffle,
}

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize))]
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-util"), derive(Eq, PartialEq))]
pub enum AlertLevel {
    Info,
    Warn,
    Error,
}

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize))]
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-util"), derive(Eq, PartialEq))]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}
