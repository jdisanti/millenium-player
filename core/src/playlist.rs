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
    message::{PlayerMessage, PlayerMessageChannel},
    metadata::Metadata,
};
use millenium_post_office::{
    broadcast::{BroadcastSubscription, Broadcaster, NoChannels},
    frontend::message::{AlertLevel, FrontendMessage, PlaylistMode},
    frontend::state::PlaybackStatus,
};
use std::{ops::Deref, str::FromStr, time::Duration};

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
#[cfg_attr(test, derive(Eq, PartialEq))]
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
#[cfg_attr(test, derive(Eq, PartialEq))]
pub struct PlaylistEntry {
    id: PlaylistEntryId,
    #[serde(skip_serializing)]
    location: Location,
    metadata: Option<MinimalMetadata>,
    duration: Option<Duration>,
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
        assert!(index.0 < self.entries.len());
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
    ui_sub: BroadcastSubscription<FrontendMessage>,
    playlist_mode: PlaylistMode,
    playback_status: Option<PlaybackStatus>,
}

impl PlaylistManager {
    pub fn new(
        player_broadcaster: Broadcaster<PlayerMessage>,
        ui_broadcaster: Broadcaster<FrontendMessage>,
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
            playlist_mode: PlaylistMode::Normal,
            playback_status: None,
        }
    }

    pub fn update(&mut self) {
        while let Some(message) = self.player_sub.try_recv() {
            #[allow(clippy::single_match)]
            match message {
                PlayerMessage::EventFinishedTrack => self.start_next_track(false),
                PlayerMessage::UpdatePlaybackStatus(status) => {
                    self.playback_status = Some(status);
                }
                _ => {}
            }
        }
        while let Some(message) = self.ui_sub.try_recv() {
            match message {
                FrontendMessage::LoadLocations { locations } => self.load_locations(
                    locations
                        .into_iter()
                        .map(|l| {
                            Location::from_str(&l).expect("frontend is only given valid locations")
                        })
                        .collect(),
                ),
                FrontendMessage::MediaControlSkipBack => self.control_skip_back(),
                FrontendMessage::MediaControlBack => log::error!("TODO: back not implemented"),
                FrontendMessage::MediaControlPause => {
                    self.player_sub.broadcast(PlayerMessage::CommandPause)
                }
                FrontendMessage::MediaControlPlay => {
                    self.player_sub.broadcast(PlayerMessage::CommandResume)
                }
                FrontendMessage::MediaControlStop => log::error!("TODO: stop not implemented"),
                FrontendMessage::MediaControlForward => {
                    log::error!("TODO: forward not implemented")
                }
                FrontendMessage::MediaControlSkipForward => self.start_next_track(true),
                FrontendMessage::MediaControlPlaylistMode { mode } => {
                    self.playlist_mode = mode;
                    // TODO: Communicate back to the UI that the playlist has changed
                }
                FrontendMessage::MediaControlSeek { position } => self
                    .player_sub
                    .broadcast(PlayerMessage::CommandSeek(position)),
                FrontendMessage::MediaControlVolume { volume } => self
                    .player_sub
                    .broadcast(PlayerMessage::CommandSetVolume(volume)),
                _ => {}
            }
        }
    }

    fn part_way_into_track(&self) -> bool {
        self.playback_status
            .map(|status| status.current_position >= Duration::from_secs(7))
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
        match self.playlist_mode {
            PlaylistMode::Normal => {
                if *current_index == 0 {
                    self.stop();
                } else {
                    self.start_track(PlaylistIndex(*current_index - 1));
                }
            }
            PlaylistMode::Shuffle => {
                unimplemented!()
            }
            PlaylistMode::RepeatOne => {
                self.restart_current_track();
            }
            PlaylistMode::RepeatAll => {
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

    fn start_next_track(&mut self, stop_immediately: bool) {
        if self.playlist.current_index.is_none() {
            return;
        }

        let (_current_id, current_index) = self.playlist.current().unwrap();
        match self.playlist_mode {
            PlaylistMode::Normal => {
                let next_index = PlaylistIndex(*current_index + 1);
                if next_index.0 >= self.playlist.entries.len() {
                    if stop_immediately {
                        self.stop();
                    } else {
                        self.playlist.clear_current();
                    }
                } else {
                    self.start_track(next_index);
                }
            }
            PlaylistMode::Shuffle => {
                unimplemented!()
            }
            PlaylistMode::RepeatOne => {
                self.restart_current_track();
            }
            PlaylistMode::RepeatAll => {
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
            self.ui_sub.broadcast(FrontendMessage::ShowAlert {
                level: AlertLevel::Info,
                message: "None of the given files are audio or playlist files.".into(),
            });
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

#[cfg(test)]
mod playlist_manager_tests {
    use super::*;

    #[test]
    fn no_entries_after_filtering() {
        let (player, ui) = (Broadcaster::new(), Broadcaster::new());
        let player_sub = player.subscribe("test", PlayerMessageChannel::All);
        let ui_sub = ui.subscribe("test", NoChannels);

        let mut manager = PlaylistManager::new(player.clone(), ui.clone());

        ui_sub.broadcast(FrontendMessage::LoadLocations {
            locations: vec![
                "not_an_audio_file1".to_string(),
                "not_an_audio_file2".to_string(),
            ],
        });
        manager.update();
        pretty_assertions::assert_eq!(Vec::<PlaylistEntry>::new(), manager.playlist.entries);
        assert_eq!(None, manager.playlist.current_id);
        assert_eq!(None, manager.playlist.current_index);
        assert_eq!(None, player_sub.try_recv());
        assert_eq!(
            Some(FrontendMessage::ShowAlert {
                level: AlertLevel::Info,
                message: "None of the given files are audio or playlist files.".into(),
            }),
            ui_sub.try_recv()
        );
    }

    #[test]
    fn normal_mode_play_all_songs_sequentially() {
        let (player, ui) = (Broadcaster::new(), Broadcaster::new());
        let player_sub = player.subscribe("test", PlayerMessageChannel::All);
        let ui_sub = ui.subscribe("test", NoChannels);

        let mut manager = PlaylistManager::new(player.clone(), ui.clone());

        ui_sub.broadcast(FrontendMessage::LoadLocations {
            locations: vec!["one.ogg".to_string(), "two.ogg".to_string()],
        });
        manager.update();
        pretty_assertions::assert_eq!(
            vec![
                PlaylistEntry {
                    id: PlaylistEntryId(1),
                    location: Location::path("one.ogg"),
                    metadata: None,
                    duration: None,
                },
                PlaylistEntry {
                    id: PlaylistEntryId(2),
                    location: Location::path("two.ogg"),
                    metadata: None,
                    duration: None,
                },
            ],
            manager.playlist.entries
        );
        assert_eq!(Some(PlaylistEntryId(1)), manager.playlist.current_id);
        assert_eq!(Some(PlaylistIndex(0)), manager.playlist.current_index);
        assert_eq!(
            PlayerMessage::CommandLoadAndPlayLocation(Location::path("one.ogg")),
            player_sub.try_recv().unwrap(),
        );

        player_sub.broadcast(PlayerMessage::EventFinishedTrack);
        manager.update();
        assert_eq!(Some(PlaylistEntryId(2)), manager.playlist.current_id);
        assert_eq!(Some(PlaylistIndex(1)), manager.playlist.current_index);
        assert_eq!(
            PlayerMessage::CommandLoadAndPlayLocation(Location::path("two.ogg")),
            player_sub.try_recv().unwrap(),
        );

        player_sub.broadcast(PlayerMessage::EventFinishedTrack);
        manager.update();
        assert_eq!(None, manager.playlist.current_id);
        assert_eq!(None, manager.playlist.current_index);
        assert_eq!(None, player_sub.try_recv());

        assert_eq!(None, ui_sub.try_recv());
    }

    #[test]
    fn normal_mode_skip_forward_to_end() {
        let (player, ui) = (Broadcaster::new(), Broadcaster::new());
        let player_sub = player.subscribe("test", PlayerMessageChannel::All);
        let ui_sub = ui.subscribe("test", NoChannels);

        let mut manager = PlaylistManager::new(player.clone(), ui.clone());

        ui_sub.broadcast(FrontendMessage::LoadLocations {
            locations: vec!["one.ogg".to_string(), "two.ogg".to_string()],
        });
        manager.update();
        assert_eq!(2, manager.playlist.entries.len());
        assert_eq!(Some(PlaylistEntryId(1)), manager.playlist.current_id);
        assert_eq!(Some(PlaylistIndex(0)), manager.playlist.current_index);
        assert_eq!(
            PlayerMessage::CommandLoadAndPlayLocation(Location::path("one.ogg")),
            player_sub.try_recv().unwrap(),
        );

        ui_sub.broadcast(FrontendMessage::MediaControlSkipForward);
        manager.update();
        assert_eq!(2, manager.playlist.entries.len());
        assert_eq!(Some(PlaylistEntryId(2)), manager.playlist.current_id);
        assert_eq!(Some(PlaylistIndex(1)), manager.playlist.current_index);
        assert_eq!(
            PlayerMessage::CommandLoadAndPlayLocation(Location::path("two.ogg")),
            player_sub.try_recv().unwrap(),
        );

        ui_sub.broadcast(FrontendMessage::MediaControlSkipForward);
        manager.update();
        assert_eq!(2, manager.playlist.entries.len());
        assert_eq!(None, manager.playlist.current_id);
        assert_eq!(None, manager.playlist.current_index);
        assert_eq!(PlayerMessage::CommandStop, player_sub.try_recv().unwrap(),);

        assert_eq!(None, player_sub.try_recv());
        assert_eq!(None, ui_sub.try_recv());
    }

    #[test]
    fn normal_mode_skip_back() {
        let (player, ui) = (Broadcaster::new(), Broadcaster::new());
        let player_sub = player.subscribe("test", PlayerMessageChannel::All);
        let ui_sub = ui.subscribe("test", NoChannels);

        let mut manager = PlaylistManager::new(player.clone(), ui.clone());

        ui_sub.broadcast(FrontendMessage::LoadLocations {
            locations: vec!["one.ogg".to_string(), "two.ogg".to_string()],
        });
        manager.update();
        assert_eq!(2, manager.playlist.entries.len());
        assert_eq!(Some(PlaylistEntryId(1)), manager.playlist.current_id);
        assert_eq!(Some(PlaylistIndex(0)), manager.playlist.current_index);
        assert_eq!(
            PlayerMessage::CommandLoadAndPlayLocation(Location::path("one.ogg")),
            player_sub.try_recv().unwrap(),
        );

        player_sub.broadcast(PlayerMessage::UpdatePlaybackStatus(PlaybackStatus {
            playing: true,
            current_position: Duration::from_secs(7),
            end_position: Some(Duration::from_secs(60)),
            volume: Default::default(),
        }));
        manager.update();

        // Since we're 7 seconds into the song, skipping back should restart the song
        ui_sub.broadcast(FrontendMessage::MediaControlSkipBack);
        manager.update();
        assert_eq!(2, manager.playlist.entries.len());
        assert_eq!(Some(PlaylistEntryId(1)), manager.playlist.current_id);
        assert_eq!(Some(PlaylistIndex(0)), manager.playlist.current_index);
        assert_eq!(
            PlayerMessage::CommandLoadAndPlayLocation(Location::path("one.ogg")),
            player_sub.try_recv().unwrap(),
        );

        // Now skipping back should go off the end of the playlist
        player_sub.broadcast(PlayerMessage::UpdatePlaybackStatus(PlaybackStatus {
            playing: true,
            current_position: Duration::from_secs(1),
            end_position: Some(Duration::from_secs(60)),
            volume: Default::default(),
        }));
        manager.update();
        ui_sub.broadcast(FrontendMessage::MediaControlSkipBack);
        manager.update();
        assert_eq!(2, manager.playlist.entries.len());
        assert_eq!(None, manager.playlist.current_id);
        assert_eq!(None, manager.playlist.current_index);
        assert_eq!(PlayerMessage::CommandStop, player_sub.try_recv().unwrap(),);

        assert_eq!(None, player_sub.try_recv());
        assert_eq!(None, ui_sub.try_recv());
    }
}
