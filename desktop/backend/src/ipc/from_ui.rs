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

use millenium_core::player::message::PlayerMessage;

use crate::ui::SharedUiResources;

pub struct MessageHandler {
    resources: SharedUiResources,
}

impl MessageHandler {
    pub fn new(resources: SharedUiResources) -> Self {
        Self { resources }
    }

    pub fn handle(&self, message: FromUiMessage) {
        log::info!("received message from UI: {:?}", message);

        let mut resources = self.resources.borrow_mut();
        use FromUiMessage::*;
        match message {
            PlayCurrent => {
                if !resources.playback_status.playing {
                    resources
                        .player()
                        .broadcaster()
                        .broadcast(PlayerMessage::CommandResume);
                    resources.playback_status.playing = true;
                }
            }
            PauseCurrent => {
                if resources.playback_status.playing {
                    resources
                        .player()
                        .broadcaster()
                        .broadcast(PlayerMessage::CommandPause);
                    resources.playback_status.playing = false;
                }
            }
            StopCurrent | SeekCurrent { .. } | LoadLocations { .. } => unimplemented!(),
            DragWindowStart | Quit => unreachable!("handled in UI"),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "kind")]
pub enum FromUiMessage {
    Quit,
    DragWindowStart,
    PlayCurrent,
    PauseCurrent,
    StopCurrent,
    SeekCurrent { position: usize },
    LoadLocations { locations: Vec<String> },
}
