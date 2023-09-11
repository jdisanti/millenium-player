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

use crate::audio::device::{
    create_device, AudioDevice, AudioDeviceMessage, AudioDeviceMessageChannel,
};
use crate::audio::sink::Sink;
use crate::message::{PlayerMessage, PlayerMessageChannel};
use crate::player::{
    state::StateManager,
    waveform::{Waveform, WaveformCalculator},
    {PlayerThreadError, PlayerThreadHandle},
};
use millenium_post_office::broadcast::{BroadcastSubscription, Broadcaster};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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
    player_sub: BroadcastSubscription<PlayerMessage>,
    device_sub: BroadcastSubscription<AudioDeviceMessage>,
}

impl PlayerThread {
    /// Creates an audio device and starts a player thread.
    fn new(
        broadcaster: Broadcaster<PlayerMessage>,
        player_sub: BroadcastSubscription<PlayerMessage>,
        preferred_output_device_name: Option<String>,
    ) -> Self {
        let device = match create_device(preferred_output_device_name.as_deref()) {
            Ok(device) => device,
            Err(err) => {
                player_sub.broadcast(PlayerMessage::EventAudioDeviceCreationFailed(
                    err.source.into(),
                ));
                err.fallback_device
            }
        };
        let device_sub = device.subscribe(
            "player-thread",
            AudioDeviceMessageChannel::Errors | AudioDeviceMessageChannel::Events,
        );

        Self {
            resources: PlayerThreadResources {
                device,
                current_sink: None,
                waveform_calculator: None,
                waveform: Arc::new(Mutex::new(Waveform::empty())),
                broadcaster: broadcaster.clone(),
            },
            player_sub,
            device_sub,
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
            while let Some(message) = self.device_sub.try_recv() {
                match message {
                    AudioDeviceMessage::Error(err) => {
                        self.player_sub
                            .broadcast(PlayerMessage::EventAudioDeviceFailed(format!("{err}")));
                        break;
                    }
                    AudioDeviceMessage::EventPlaybackFinished => {}
                    AudioDeviceMessage::EventAudioDeviceIdle => {
                        self.resources.device.pause().unwrap();
                    }
                    _ => {}
                }
            }

            let next_message = if state_manager.blocked_on_messages() {
                // Use a timeout so that audio device messages are still handled
                self.player_sub.recv_timeout(Duration::from_millis(500))
            } else {
                self.player_sub.try_recv()
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
    #[ntest::timeout(500)]
    fn spawn_and_close() {
        let handle = PlayerThread::spawn(None).unwrap();
        handle.broadcaster().broadcast(PlayerMessage::CommandQuit);
        handle.join().expect("success");
    }
}
