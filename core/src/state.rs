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
    broadcast::{BroadcastMessage, BroadcastSubscription, Broadcaster, NoChannels},
    message::PlaybackStatus,
    metadata::Metadata,
    player::waveform::Waveform,
    playlist::PlaylistMode,
};
use std::{cell::RefCell, ops::Deref, rc::Rc};

#[derive(Copy, Clone, Debug)]
pub struct StateChanged;

impl BroadcastMessage for StateChanged {
    type Channel = NoChannels;

    fn channel(&self) -> Self::Channel {
        NoChannels
    }

    fn frequent(&self) -> bool {
        true
    }
}

#[derive(Debug)]
pub struct State {
    pub metadata: Option<Metadata>,
    pub playback_status: PlaybackStatus,
    pub playlist_mode: PlaylistMode,
    pub waveform: Waveform,
}

impl State {
    pub fn new_handle() -> StateHandle {
        StateHandle {
            state: Rc::new(RefCell::new(Self {
                metadata: None,
                playback_status: PlaybackStatus::default(),
                playlist_mode: PlaylistMode::Normal,
                waveform: Waveform::empty(),
            })),
            broadcaster: Broadcaster::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct StateHandle {
    state: Rc<RefCell<State>>,
    broadcaster: Broadcaster<StateChanged>,
}

impl StateHandle {
    pub fn subscribe(&self, name: &'static str) -> BroadcastSubscription<StateChanged> {
        self.broadcaster.subscribe(name, NoChannels)
    }

    pub fn borrow(&self) -> impl Deref<Target = State> + '_ {
        self.state.borrow()
    }

    pub fn mutate(&self, f: impl FnOnce(&mut State)) {
        f(&mut self.state.borrow_mut());
        self.broadcaster.broadcast(StateChanged);
    }
}
