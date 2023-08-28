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

use super::audio::{CpalAudioDevice, NullAudioDevice};
use super::sink::Sink;
use super::source::AudioSource;
use super::{message, PlayerThreadError, PlayerThreadHandle};
use crate::location::Location;
use crate::player::audio::AudioDevice;
use crate::player::message::{FromPlayerMessage, ToPlayerMessage};
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
    device: Box<dyn AudioDevice>,
    current_sink: Option<Sink>,
    to_rx: mpsc::Receiver<message::ToPlayerMessage>,
    from_tx: mpsc::Sender<message::FromPlayerMessage>,
}

impl PlayerThread {
    fn new(
        to_rx: mpsc::Receiver<message::ToPlayerMessage>,
        from_tx: mpsc::Sender<message::FromPlayerMessage>,
        preferred_output_device_name: Option<String>,
    ) -> Self {
        let device = CpalAudioDevice::new(preferred_output_device_name.as_deref())
            .map(|d| Box::new(d) as Box<dyn AudioDevice>)
            .unwrap_or_else(|err| {
                log::error!("{err}");
                let _ = from_tx.send(FromPlayerMessage::AudioDeviceCreationFailed(err));
                Box::new(NullAudioDevice::new())
            });

        Self {
            device,
            current_sink: None,
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

    fn run(mut self) {
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

    fn handle_state(&mut self, state: State) -> State {
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
                self.device.pause().expect("failed to pause audio stream");
                let state = if let Some(new_state) = self.queue_chunks(&mut source) {
                    new_state
                } else {
                    State::Playing(source)
                };
                self.device.play().expect("failed to pause audio stream");
                state
            }
            State::Playing(mut source) => {
                let next_state = if let Some(new_state) = self.queue_chunks(&mut source) {
                    new_state
                } else {
                    State::Playing(source)
                };
                if let Some(sink) = self.current_sink.as_mut() {
                    sink.send_audio_with_timeout(Duration::from_millis(50));
                }
                next_state
            }
            state => state,
        }
    }

    fn queue_chunks(&mut self, source: &mut AudioSource) -> Option<State> {
        while self
            .current_sink
            .as_ref()
            .map(|s| s.needs_more_chunks())
            .unwrap_or(true)
        {
            match source.next_chunk() {
                Ok(Some(chunk)) => {
                    if chunk.frame_count() > 0 {
                        let sample_rate = chunk.sample_rate();
                        let channels = chunk.channel_count();
                        let recreate_sink = match &self.current_sink {
                            Some(sink) => {
                                sink.input_channels() != channels
                                    || sink.input_sample_rate() != sample_rate
                            }
                            None => true,
                        };
                        if recreate_sink {
                            log::info!("recreating the audio sink");
                            if let Some(s) = self.current_sink.as_mut() {
                                s.flush()
                            }
                            self.current_sink =
                                Some(self.device.create_sink(sample_rate, channels));
                        }
                        self.current_sink.as_mut().unwrap().queue(chunk);
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
