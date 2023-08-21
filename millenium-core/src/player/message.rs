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

use super::source::AudioSourceError;
use crate::{location::Location, metadata::Metadata};

pub enum FromPlayerMessage {
    /// This is the loaded track metadata.
    MetadataLoaded(Metadata),
    /// The currently playing track finished.
    FinishedTrack,
    /// Failed to load location.
    FailedToLoadLocation(AudioSourceError),
    /// Failed to decode audio.
    FailedToDecodeAudio(AudioSourceError),
    /// The audio device failed.
    AudioDeviceFailed(String),
}

pub enum ToPlayerMessage {
    /// The application is shutting down. Exit the player thread.
    Quit,
    /// Load and play a location.
    LoadAndPlayLocation(Location),
    /// Pause playback.
    Pause,
    /// Resume playback.
    Resume,
    /// Stop playback.
    Stop,
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<super::ToPlayerMessage>();
        assert_send::<super::FromPlayerMessage>();
    }
}
