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

use super::message::{PlayerMessage, PlayerMessageChannel};
use super::state::StateManager;
use super::waveform::{Waveform, WaveformCalculator};
use super::{PlayerThreadError, PlayerThreadHandle};
use crate::audio::device::{create_device, AudioDevice};
use crate::audio::sink::Sink;
use crate::broadcast::{BroadcastSubscription, Broadcaster};
use std::sync::{Arc, Mutex};
use std::thread;

pub(super) struct PlayerThreadResources {
    pub(super) device: Box<dyn AudioDevice>,
    pub(super) current_sink: Option<Sink>,
    pub(super) waveform_calculator: Option<WaveformCalculator>,
    pub(super) waveform: Arc<Mutex<Waveform>>,
    pub(super) broadcaster: Broadcaster<PlayerMessage>,
}

/// Audio playback thread.
pub struct PlayerThread {
    resources: PlayerThreadResources,
    subscription: BroadcastSubscription<PlayerMessage>,
}

impl PlayerThread {
    /// Creates an audio device and starts a player thread.
    fn new(
        broadcaster: Broadcaster<PlayerMessage>,
        subscription: BroadcastSubscription<PlayerMessage>,
        preferred_output_device_name: Option<String>,
    ) -> Self {
        let device = match create_device(preferred_output_device_name.as_deref()) {
            Ok(device) => device,
            Err(err) => {
                broadcaster.broadcast(PlayerMessage::EventAudioDeviceCreationFailed(
                    err.source.into(),
                ));
                err.fallback_device
            }
        };

        Self {
            resources: PlayerThreadResources {
                device,
                current_sink: None,
                waveform_calculator: None,
                waveform: Arc::new(Mutex::new(Waveform::empty())),
                broadcaster: broadcaster.clone(),
            },
            subscription,
        }
    }

    pub fn spawn(
        preferred_output_device_name: Option<String>,
    ) -> Result<PlayerThreadHandle, PlayerThreadError> {
        let broadcaster = Broadcaster::new();
        let subscription = broadcaster.subscribe("player-thread", PlayerMessageChannel::Commands);
        let join_handle = thread::Builder::new()
            .name("player".into())
            .spawn({
                let broadcaster = broadcaster.clone();
                move || {
                    PlayerThread::new(broadcaster, subscription, preferred_output_device_name)
                        .run();
                }
            })
            .map_err(|source| PlayerThreadError::FailedToSpawn { source })?;
        Ok(PlayerThreadHandle::new(join_handle, broadcaster))
    }

    fn run(mut self) {
        log::info!("player thread started");

        let mut state_manager = StateManager::new();
        while !state_manager.should_quit() {
            if let Err(err) = self.resources.device.healthcheck() {
                self.resources
                    .broadcaster
                    .broadcast(PlayerMessage::EventAudioDeviceFailed(format!("{err}")));
                break;
            }

            let next_message = if state_manager.blocked_on_messages() {
                self.subscription.recv()
            } else {
                self.subscription.try_recv()
            };
            if let Some(message) = next_message {
                log::info!("player received message: {message:?}");
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
    #[ntest::timeout(100)]
    fn spawn_and_close() {
        let handle = PlayerThread::spawn(None).unwrap();
        handle.broadcaster().broadcast(PlayerMessage::CommandQuit);
        handle.join().expect("success");
    }
}
