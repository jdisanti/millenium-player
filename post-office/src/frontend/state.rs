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

use std::time::Duration;

pub use crate::frontend::message::PlaylistMode;
use crate::types::Volume;

#[cfg(feature = "broadcast")]
pub type PlaybackState = crate::state::State<PlaybackStateData>;
#[cfg(feature = "broadcast")]
pub type WaveformState = crate::state::State<WaveformStateData>;

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize))]
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
pub struct PlaybackStateData {
    pub current_track: Option<Track>,
    pub playback_status: PlaybackStatus,
    pub playlist_mode: PlaylistMode,
}

impl Default for PlaybackStateData {
    fn default() -> Self {
        Self {
            current_track: None,
            playback_status: PlaybackStatus::default(),
            playlist_mode: PlaylistMode::Normal,
        }
    }
}

#[derive(Default, Debug, PartialEq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize))]
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
pub struct Track {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
}

impl Track {
    pub fn empty() -> Self {
        Self {
            title: None,
            artist: None,
            album: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize))]
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
pub struct PlaybackStatus {
    pub playing: bool,
    pub position_secs: Duration,
    pub duration_secs: Option<Duration>,
    pub volume: Volume,
}

#[derive(Debug, Default, PartialEq)]
pub struct WaveformStateData {
    pub waveform: Option<Waveform>,
}

#[derive(Debug, PartialEq)]
pub struct Waveform {
    pub spectrum: Box<[f32]>,
    pub amplitude: Box<[f32]>,
}
