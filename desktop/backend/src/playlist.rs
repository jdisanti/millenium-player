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

use millenium_core::{
    broadcast::{BroadcastSubscription, Broadcaster, NoChannels},
    location::Location,
    metadata::Metadata,
    player::message::{PlaybackStatus, PlayerMessage, PlayerMessageChannel},
};
use std::{ops::Deref, time::Duration};

use crate::ui::UiMessage;

#[derive(Copy, Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct PlaylistEntryId(usize);

impl Deref for PlaylistEntryId {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct PlaylistIndex(usize);

impl Deref for PlaylistIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct MinimalMetadata {
    artist: Option<String>,
    album_artist: Option<String>,
    title: Option<String>,
}

impl From<&Metadata> for MinimalMetadata {
    fn from(value: &Metadata) -> Self {
        MinimalMetadata {
            artist: value.artist.clone(),
            album_artist: value.album_artist.clone(),
            title: value.track_title.clone(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct PlaylistEntry {
    id: PlaylistEntryId,
    #[serde(skip_serializing)]
    location: Location,
    metadata: Option<MinimalMetadata>,
    duration: Option<Duration>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub enum PlaybackMode {
    Normal,
    RepeatOne,
    RepeatAll,
    Shuffle,
}

#[derive(Default)]
pub struct Playlist {
    entries: Vec<PlaylistEntry>,
    current_id: Option<PlaylistEntryId>,
    current_index: Option<PlaylistIndex>,
}

impl Playlist {
    pub fn clear_current(&mut self) {
        self.current_id = None;
        self.current_index = None;
    }

    pub fn set_current_index(&mut self, index: PlaylistIndex) {
        self.current_index = Some(index);
        self.current_id = Some(self.entries[index.0].id);
    }

    pub fn current(&self) -> Option<(PlaylistEntryId, PlaylistIndex)> {
        self.current_id.zip(self.current_index)
    }
}

pub struct PlaylistManager {
    next_id: usize,
    playlist: Playlist,
    player_sub: BroadcastSubscription<PlayerMessage>,
    ui_sub: BroadcastSubscription<UiMessage>,
    playback_mode: PlaybackMode,
    playback_status: Option<PlaybackStatus>,
}

impl PlaylistManager {
    pub fn new(
        player_broadcaster: Broadcaster<PlayerMessage>,
        ui_broadcaster: Broadcaster<UiMessage>,
    ) -> Self {
        let player_sub = player_broadcaster.subscribe(
            "playlist-manager",
            PlayerMessageChannel::Events | PlayerMessageChannel::FrequentUpdates,
        );
        let ui_sub = ui_broadcaster.subscribe("playlist-manager", NoChannels);
        Self {
            next_id: 0,
            playlist: Playlist::default(),
            player_sub,
            ui_sub,
            playback_mode: PlaybackMode::Normal,
            playback_status: None,
        }
    }

    pub fn update(&mut self) {
        while let Some(message) = self.player_sub.try_recv() {
            #[allow(clippy::single_match)]
            match message {
                PlayerMessage::EventFinishedTrack => self.start_next_track(),
                PlayerMessage::UpdatePlaybackStatus(status) => {
                    self.playback_status = Some(status);
                }
                _ => {}
            }
        }
        while let Some(message) = self.ui_sub.try_recv() {
            match message {
                UiMessage::LoadLocations { locations } => self.load_locations(locations),
                UiMessage::MediaControlSkipBack => self.control_skip_back(),
                UiMessage::MediaControlBack => log::error!("back not implemented"),
                UiMessage::MediaControlPause => {
                    self.player_sub.broadcast(PlayerMessage::CommandPause)
                }
                UiMessage::MediaControlPlay => {
                    self.player_sub.broadcast(PlayerMessage::CommandResume)
                }
                UiMessage::MediaControlStop => log::error!("stop not implemented"),
                UiMessage::MediaControlForward => log::error!("forward not implemented"),
                UiMessage::MediaControlSkipForward => self.start_next_track(),
                _ => {}
            }
        }
    }

    fn part_way_into_track(&self) -> bool {
        self.playback_status
            .map(|status| status.position_secs > 7.0)
            .unwrap_or(false)
    }

    fn next_id(&mut self) -> PlaylistEntryId {
        self.next_id += 1;
        PlaylistEntryId(self.next_id)
    }

    fn control_skip_back(&mut self) {
        if self.part_way_into_track() {
            self.restart_current_track();
        } else {
            self.start_previous_track();
        }
    }

    fn restart_current_track(&mut self) {
        if let Some(current_index) = self.playlist.current_index {
            self.start_track(current_index);
        }
    }

    fn start_previous_track(&mut self) {
        if self.playlist.current_index.is_none() {
            return;
        }

        let (_current_id, current_index) = self.playlist.current().unwrap();
        match self.playback_mode {
            PlaybackMode::Normal => {
                if *current_index == 0 {
                    self.stop();
                } else {
                    self.start_track(PlaylistIndex(*current_index - 1));
                }
            }
            PlaybackMode::Shuffle => {
                unimplemented!()
            }
            PlaybackMode::RepeatOne => {
                self.restart_current_track();
            }
            PlaybackMode::RepeatAll => {
                unimplemented!()
            }
        }
    }

    fn stop(&mut self) {
        self.playlist.clear_current();
        self.player_sub.broadcast(PlayerMessage::CommandStop);
    }

    fn start_track(&mut self, index: PlaylistIndex) {
        self.playlist.set_current_index(index);
        self.player_sub
            .broadcast(PlayerMessage::CommandLoadAndPlayLocation(
                self.playlist.entries[index.0].location.clone(),
            ));
    }

    fn start_next_track(&mut self) {
        if self.playlist.current_index.is_none() {
            return;
        }

        let (_current_id, current_index) = self.playlist.current().unwrap();
        match self.playback_mode {
            PlaybackMode::Normal => {
                let next_index = PlaylistIndex(*current_index + 1);
                if next_index.0 >= self.playlist.entries.len() {
                    self.stop();
                } else {
                    self.start_track(next_index);
                }
            }
            PlaybackMode::Shuffle => {
                unimplemented!()
            }
            PlaybackMode::RepeatOne => {
                self.restart_current_track();
            }
            PlaybackMode::RepeatAll => {
                unimplemented!()
            }
        }
    }

    fn load_locations(&mut self, locations: Vec<Location>) {
        let filtered_locations: Vec<Location> = locations
            .iter()
            .cloned()
            .filter(|location| !location.inferred_type().is_unknown())
            // TODO: remove the following filter and load playlists
            .filter(|location| !location.inferred_type().is_playlist())
            .collect();
        if filtered_locations.is_empty() && !locations.is_empty() {
            rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("Error")
                .set_description("None of the given files are audio or playlist files.")
                .show();
        }
        let entries: Vec<PlaylistEntry> = filtered_locations
            .into_iter()
            .map(|location| {
                PlaylistEntry {
                    id: self.next_id(),
                    location,
                    // TODO: Add support for metadata loading
                    metadata: None,
                    duration: None,
                }
            })
            .collect();
        let (current_id, current_index) = if let Some(first) = entries.first() {
            (Some(first.id), Some(PlaylistIndex(0)))
        } else {
            (None, None)
        };

        self.playlist = Playlist {
            entries,
            current_id,
            current_index,
        };

        if current_id.is_some() {
            let entry = &self.playlist.entries[0];
            self.player_sub
                .broadcast(PlayerMessage::CommandLoadAndPlayLocation(
                    entry.location.clone(),
                ));
        }
    }
}
