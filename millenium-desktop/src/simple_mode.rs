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
    location::Location,
    player::{
        message::{FromPlayerMessage, ToPlayerMessage},
        PlayerThread, PlayerThreadHandle,
    },
};
use std::{
    error::Error as StdError,
    sync::mpsc::{self, Receiver},
};

enum State {
    NoLocations,
    PlayStream(PlayStreamState),
}

#[derive(Default)]
struct PlayStreamState {}

struct Playlist {
    locations: Vec<Location>,
    current: Option<usize>,
}

pub struct SimpleMode {
    player: Option<PlayerThreadHandle>,
    // TODO receive and handle messages from player thread
    _player_receiver: Receiver<FromPlayerMessage>,
    _playlist: Playlist,
    _state: State,
}

impl SimpleMode {
    fn new(locations: &[Location]) -> Result<Self, Box<dyn StdError>> {
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
        let (player_sender, player_receiver) = mpsc::channel();
        let player = PlayerThread::spawn(player_sender, None)?;
        let playlist = Playlist {
            current: filtered_locations.get(0).map(|_| Some(0)).unwrap_or(None),
            locations: filtered_locations,
        };
        let state = if let Some(index) = playlist.current {
            player.send(ToPlayerMessage::LoadAndPlayLocation(
                playlist.locations[index].clone(),
            ))?;
            State::PlayStream(Default::default())
        } else {
            State::NoLocations
        };
        Ok(Self {
            player: Some(player),
            _player_receiver: player_receiver,
            _playlist: playlist,
            _state: state,
        })
    }

    pub fn run(locations: &[Location]) -> Result<(), Box<dyn StdError>> {
        let native_options = eframe::NativeOptions {
            app_id: Some("millenium-player".into()),
            drag_and_drop_support: true,
            resizable: false,
            ..Default::default()
        };
        let app = Box::new(Self::new(locations)?);
        eframe::run_native(
            "Millenium Player",
            native_options,
            Box::new(move |_creation_context| app),
        )?;
        Ok(())
    }

    fn healthcheck(&mut self, frame: &mut eframe::Frame) {
        match self.player.take().unwrap().healthcheck() {
            Ok(player) => {
                self.player = Some(player);
            }
            Err(err) => {
                // TODO: display error
                log::error!("{err}");
                frame.close();
            }
        }
    }
}

impl eframe::App for SimpleMode {
    fn update(&mut self, _ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.healthcheck(frame);
    }

    fn on_close_event(&mut self) -> bool {
        if let Some(player) = self.player.as_ref() {
            let _ = player.send(ToPlayerMessage::Quit);
        }
        true
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(player) = self.player.take() {
            if let Err(err) = player.join() {
                log::error!("{err}");
            }
        }
    }
}
