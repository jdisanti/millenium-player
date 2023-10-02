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

use crate::audio::{device::AudioDeviceError, source::AudioSourceError};
use crate::player::waveform::Waveform;
use crate::{location::Location, metadata::Metadata};
use millenium_post_office::{
    broadcast::{BroadcastMessage, Channel},
    frontend::state::PlaybackStatus,
    types::Volume,
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

bitflags::bitflags! {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct PlayerMessageChannel: u8 {
        const All = 0xFF;
        const Events = 0x01;
        const Commands = 0x02;
        const FrequentUpdates = 0x04;
    }
}

impl Channel for PlayerMessageChannel {
    fn matches(&self, other: Self) -> bool {
        self.bits() & other.bits() != 0
    }
}

#[derive(Clone, Debug)]
pub enum PlayerMessage {
    /// The application is shutting down. Exit the player thread.
    CommandQuit,
    /// Load and play a location.
    CommandLoadAndPlayLocation(Location),
    /// Pause playback.
    CommandPause,
    /// Resume playback.
    CommandResume,
    /// Stop playback.
    CommandStop,
    /// Seek to a position in the currently playing track.
    CommandSeek(Duration),
    /// Change the playback volume.
    CommandSetVolume(Volume),

    /// This is the loaded track metadata.
    EventMetadataLoaded(Metadata),
    /// The currently playing track started.
    EventStartedTrack,
    /// The currently playing track finished.
    EventFinishedTrack,
    /// Failed to load location.
    EventFailedToLoadLocation(Arc<AudioSourceError>),
    /// Failed to decode audio.
    EventFailedToDecodeAudio(Arc<AudioSourceError>),
    /// The audio device failed.
    EventAudioDeviceFailed(String),
    /// Failed to create an audio device.
    EventAudioDeviceCreationFailed(Arc<AudioDeviceError>),

    /// The playback status changed.
    UpdatePlaybackStatus(PlaybackStatus),
    /// Updated waveform data.
    UpdateWaveform(Arc<Mutex<Waveform>>),
}

impl BroadcastMessage for PlayerMessage {
    type Channel = PlayerMessageChannel;

    fn channel(&self) -> Self::Channel {
        match self {
            Self::CommandQuit
            | Self::CommandLoadAndPlayLocation(_)
            | Self::CommandPause
            | Self::CommandResume
            | Self::CommandStop
            | Self::CommandSeek(_)
            | Self::CommandSetVolume(_) => Self::Channel::Commands,

            Self::EventMetadataLoaded(_)
            | Self::EventStartedTrack
            | Self::EventFinishedTrack
            | Self::EventFailedToLoadLocation(_)
            | Self::EventFailedToDecodeAudio(_)
            | Self::EventAudioDeviceFailed(_)
            | Self::EventAudioDeviceCreationFailed(_) => Self::Channel::Events,

            Self::UpdatePlaybackStatus(_) | Self::UpdateWaveform(_) => {
                Self::Channel::FrequentUpdates
            }
        }
    }

    fn frequent(&self) -> bool {
        self.channel() == PlayerMessageChannel::FrequentUpdates
    }
}

#[cfg(feature = "test-util")]
impl PartialEq for PlayerMessage {
    fn eq(&self, other: &Self) -> bool {
        use PlayerMessage::*;
        match (self, other) {
            (CommandQuit, CommandQuit) => true,
            (CommandLoadAndPlayLocation(l), CommandLoadAndPlayLocation(r)) => l == r,
            (CommandPause, CommandPause) => true,
            (CommandResume, CommandResume) => true,
            (CommandStop, CommandStop) => true,
            (CommandSeek(a), CommandSeek(b)) => a == b,
            (CommandSetVolume(a), CommandSetVolume(b)) => a == b,

            (EventMetadataLoaded(l), EventMetadataLoaded(r)) => l == r,
            (EventStartedTrack, EventStartedTrack) => true,
            (EventFinishedTrack, EventFinishedTrack) => true,

            (UpdatePlaybackStatus(l), UpdatePlaybackStatus(r)) => l == r,

            (UpdateWaveform(_), UpdateWaveform(_))
            | (EventAudioDeviceCreationFailed(_), EventAudioDeviceCreationFailed(_))
            | (EventFailedToLoadLocation(_), EventFailedToLoadLocation(_))
            | (EventFailedToDecodeAudio(_), EventFailedToDecodeAudio(_))
            | (EventAudioDeviceFailed(_), EventAudioDeviceFailed(_)) => {
                core::mem::discriminant(self) == core::mem::discriminant(other)
            }

            _ => false,
        }
    }
}
