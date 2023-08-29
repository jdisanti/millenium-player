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

use std::sync::mpsc;

mod handle;
pub mod message;
mod thread;

pub use handle::PlayerThreadHandle;
pub use thread::PlayerThread;

#[derive(Debug, thiserror::Error)]
pub enum PlayerThreadError {
    #[error("player thread exited early")]
    EarlyExit,
    #[error("failed to join player thread: {panic_reason}")]
    FailedToJoin { panic_reason: String },
    #[error("failed to join player thread: no panic reason given")]
    FailedToJoinNoReason,
    #[error("failed to spawn player thread: {source}")]
    FailedToSpawn {
        #[source]
        source: std::io::Error,
    },
    #[error("failed to send message to player thread: {source}")]
    FailedToSendMessage {
        #[source]
        #[from]
        source: mpsc::SendError<message::ToPlayerMessage>,
    },
}
