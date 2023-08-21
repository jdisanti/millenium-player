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

use crate::location::Location;
use crate::player::message::{FromPlayerMessage, ToPlayerMessage};

use super::audio::AudioDevice;
use super::source::AudioSource;
use super::{message, PlayerThreadError, PlayerThreadHandle};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

enum State {
    DoNothing,
    Quit,
    LoadLocation(Location),
    Playing(AudioSource),
    Paused(AudioSource),
}

impl State {
    fn blocked(&self) -> bool {
        match self {
            Self::Quit => false,
            Self::DoNothing | Self::Paused(_) => true,
            Self::LoadLocation(_) | Self::Playing(_) => false,
        }
    }
}

pub struct PlayerThread {
    device: AudioDevice,
    to_rx: mpsc::Receiver<message::ToPlayerMessage>,
    from_tx: mpsc::Sender<message::FromPlayerMessage>,
}

impl PlayerThread {
    fn new(
        to_rx: mpsc::Receiver<message::ToPlayerMessage>,
        from_tx: mpsc::Sender<message::FromPlayerMessage>,
        preferred_output_device_name: Option<String>,
    ) -> Self {
        let device = AudioDevice::new(preferred_output_device_name.as_deref())
            .expect("failed to create device");

        Self {
            device,
            to_rx,
            from_tx,
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

    fn send(&self, message: FromPlayerMessage) {
        self.from_tx
            .send(message)
            .expect("failed to send message back to owner of the player thread");
    }

    fn run(self) {
        log::info!("player thread started");

        let mut state = Some(State::DoNothing);
        while !matches!(state, Some(State::Quit)) {
            if let Err(err) = self.device.healthcheck() {
                self.send(FromPlayerMessage::AudioDeviceFailed(format!("{err}")));
                break;
            }

            let next_message = if state.as_ref().unwrap().blocked() {
                self.to_rx.recv().ok()
            } else {
                self.to_rx.try_recv().ok()
            };
            if let Some(message) = next_message {
                self.handle_message(&mut state, message);
            }
            state = Some(self.handle_state(state.take().unwrap()));
        }
        log::info!("player thread finished");
    }

    fn handle_message(&self, state: &mut Option<State>, message: ToPlayerMessage) {
        match message {
            ToPlayerMessage::Quit => {
                *state = Some(State::Quit);
            }
            ToPlayerMessage::LoadAndPlayLocation(location) => {
                *state = Some(State::LoadLocation(location));
            }
            ToPlayerMessage::Pause => {
                if let Some(State::Playing(source)) = state.take() {
                    log::info!("pausing playback");
                    *state = Some(State::Paused(source));
                }
            }
            ToPlayerMessage::Resume => {
                if let Some(State::Paused(source)) = state.take() {
                    log::info!("resuming playback");
                    *state = Some(State::Playing(source));
                }
            }
            ToPlayerMessage::Stop => {
                log::info!("stopping playback");
                *state = Some(State::DoNothing);
            }
        }
    }

    fn handle_state(&self, state: State) -> State {
        match state {
            State::LoadLocation(location) => {
                log::info!("loading location: {:?}", location);
                let mut source = match AudioSource::new(location) {
                    Ok(source) => source,
                    Err(err) => {
                        log::error!("failed to load location: {}", err);
                        self.send(FromPlayerMessage::FailedToLoadLocation(err));
                        return State::DoNothing;
                    }
                };
                if let Some(metadata) = source.metadata() {
                    log::info!("loaded metadata: {:?}", metadata);
                    self.send(FromPlayerMessage::MetadataLoaded(metadata.clone()));
                }
                self.device.pause();
                let state = if let Some(new_state) = self.queue_chunks(&mut source) {
                    new_state
                } else {
                    State::Playing(source)
                };
                self.device.play();
                state
            }
            State::Playing(mut source) => {
                if self.device.needs_more_chunks() {
                    if let Some(new_state) = self.queue_chunks(&mut source) {
                        new_state
                    } else {
                        State::Playing(source)
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(50));
                    State::Playing(source)
                }
            }
            state => state,
        }
    }

    fn queue_chunks(&self, source: &mut AudioSource) -> Option<State> {
        while self.device.needs_more_chunks() {
            match source.next_chunk() {
                Ok(Some(chunk)) => {
                    if chunk.frames() > 0 {
                        self.device.queue_chunk(&chunk);
                    }
                }
                Ok(None) => {
                    log::info!("finished playing track");
                    self.send(FromPlayerMessage::FinishedTrack);
                    return Some(State::DoNothing);
                }
                Err(err) => {
                    log::error!("error occurred while decoding audio: {}", err);
                    self.send(FromPlayerMessage::FailedToDecodeAudio(err));
                    return Some(State::DoNothing);
                }
            }
        }
        None
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
