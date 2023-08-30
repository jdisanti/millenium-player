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

use super::state::StateManager;
use super::{message, PlayerThreadError, PlayerThreadHandle};
use crate::audio::device::{create_device, AudioDevice};
use crate::audio::sink::Sink;
use crate::player::message::FromPlayerMessage;
use std::sync::mpsc;
use std::thread;

pub(super) struct PlayerThreadResources {
    pub(super) device: Box<dyn AudioDevice>,
    pub(super) current_sink: Option<Sink>,
    from_tx: mpsc::Sender<message::FromPlayerMessage>,
}

impl PlayerThreadResources {
    pub(super) fn send_message(&self, message: FromPlayerMessage) {
        self.from_tx
            .send(message)
            .expect("failed to send message back to owner of the player thread");
    }
}

/// Audio playback thread.
pub struct PlayerThread {
    resources: PlayerThreadResources,
    to_rx: mpsc::Receiver<message::ToPlayerMessage>,
}

impl PlayerThread {
    /// Creates an audio device and starts a player thread.
    fn new(
        to_rx: mpsc::Receiver<message::ToPlayerMessage>,
        from_tx: mpsc::Sender<message::FromPlayerMessage>,
        preferred_output_device_name: Option<String>,
    ) -> Self {
        let device = match create_device(preferred_output_device_name.as_deref()) {
            Ok(device) => device,
            Err(err) => {
                let _ = from_tx.send(FromPlayerMessage::AudioDeviceCreationFailed(err.source));
                err.fallback_device
            }
        };

        Self {
            resources: PlayerThreadResources {
                device,
                current_sink: None,
                from_tx,
            },
            to_rx,
        }
    }

    pub fn spawn(
        from_tx: mpsc::Sender<message::FromPlayerMessage>,
        preferred_output_device_name: Option<String>,
    ) -> Result<PlayerThreadHandle, PlayerThreadError> {
        let (to_tx, to_rx) = mpsc::channel();
        let join_handle = thread::Builder::new()
            .name("player".into())
            .spawn(move || {
                PlayerThread::new(to_rx, from_tx, preferred_output_device_name).run();
            })
            .map_err(|source| PlayerThreadError::FailedToSpawn { source })?;
        Ok(PlayerThreadHandle::new(join_handle, to_tx))
    }

    fn run(mut self) {
        log::info!("player thread started");

        let mut state_manager = StateManager::new();
        while !state_manager.should_quit() {
            if let Err(err) = self.resources.device.healthcheck() {
                self.resources
                    .send_message(FromPlayerMessage::AudioDeviceFailed(format!("{err}")));
                break;
            }

            let next_message = if state_manager.blocked_on_messages() {
                self.to_rx.recv().ok()
            } else {
                self.to_rx.try_recv().ok()
            };
            if let Some(message) = next_message {
                state_manager.handle_message(&mut self.resources, message);
            }
            state_manager.update(&mut self.resources);
        }
        log::info!("player thread finished");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_and_close() {
        let (from_tx, _) = mpsc::channel();
        let handle = PlayerThread::spawn(from_tx, None).unwrap();
        handle.send(message::ToPlayerMessage::Quit).unwrap();
        handle.join().expect("success");
    }
}
