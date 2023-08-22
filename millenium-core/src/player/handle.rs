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

use super::{message, PlayerThreadError};
use std::any::Any;
use std::sync::mpsc;
use std::thread;

pub struct PlayerThreadHandle {
    handle: thread::JoinHandle<()>,
    to_tx: mpsc::Sender<message::ToPlayerMessage>,
}

impl PlayerThreadHandle {
    pub(super) fn new(
        handle: thread::JoinHandle<()>,
        to_tx: mpsc::Sender<message::ToPlayerMessage>,
    ) -> Self {
        Self { handle, to_tx }
    }

    pub fn healthcheck(self) -> Result<Self, PlayerThreadError> {
        if self.handle.is_finished() {
            if let Err(err) = self.join() {
                Err(err)
            } else {
                Err(PlayerThreadError::EarlyExit)
            }
        } else {
            Ok(self)
        }
    }

    pub fn send(&self, message: message::ToPlayerMessage) -> Result<(), PlayerThreadError> {
        self.to_tx.send(message)?;
        Ok(())
    }

    pub fn join(self) -> Result<(), PlayerThreadError> {
        self.handle.join().map_err(Self::map_join_err)?;
        Ok(())
    }

    fn map_join_err(panic_reason: Box<dyn Any + Send>) -> PlayerThreadError {
        let panic_reason = panic_reason
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or(panic_reason.downcast_ref::<String>().cloned());
        if let Some(panic_reason) = panic_reason {
            PlayerThreadError::FailedToJoin { panic_reason }
        } else {
            PlayerThreadError::FailedToJoinNoReason
        }
    }
}
