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

use crate::broadcast::{BroadcastMessage, BroadcastSubscription, Broadcaster, NoChannels};
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

#[derive(Debug, Default)]
pub struct State<S> {
    state: Rc<RefCell<S>>,
    broadcaster: Broadcaster<StateChanged>,
}

// Have to manually implement this because we don't want to enforce a Clone bound on S
impl<S> Clone for State<S> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            broadcaster: self.broadcaster.clone(),
        }
    }
}

impl<S> State<S>
where
    S: Default,
{
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(S::default())),
            broadcaster: Broadcaster::new(),
        }
    }

    pub fn subscribe(&self, name: &'static str) -> BroadcastSubscription<StateChanged> {
        self.broadcaster.subscribe(name, NoChannels)
    }

    pub fn borrow(&self) -> impl Deref<Target = S> + '_ {
        self.state.borrow()
    }

    pub fn mutate(&self, f: impl FnOnce(&mut S)) {
        f(&mut self.state.borrow_mut());
        self.broadcaster.broadcast(StateChanged);
    }
}
