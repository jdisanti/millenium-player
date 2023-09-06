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
    audio::source::{AudioDecoderSource, PreferredFormat},
    location::Location,
    player::{
        message::{PlaybackStatus, PlayerMessage},
        thread::PlayerThreadResources,
        waveform::WaveformCalculator,
    },
};
use std::{
    mem,
    time::{Duration, Instant},
};

trait State {
    fn update(self, resources: &mut PlayerThreadResources) -> CurrentState;
}

#[derive(Default)]
enum CurrentState {
    #[default]
    DoNothing,
    Quit,
    LoadLocation(StateLoadLocation),
    Playing(StatePlaying),
    Paused(StatePlaying),
}

impl CurrentState {
    fn handle_message(self, resources: &PlayerThreadResources, message: PlayerMessage) -> Self {
        match message {
            PlayerMessage::CommandQuit => CurrentState::Quit,
            PlayerMessage::CommandPause => {
                if let CurrentState::Playing(state) = self {
                    state.transition_to_pause_state(resources)
                } else {
                    self
                }
            }
            PlayerMessage::CommandResume => {
                if matches!(self, CurrentState::Paused(_)) {
                    log::info!("resuming playback");
                    let CurrentState::Paused(state) = self else { unreachable!() };
                    resources.device.play().unwrap();
                    CurrentState::Playing(state)
                } else {
                    self
                }
            }
            PlayerMessage::CommandStop => {
                log::info!("stopping playback");
                if matches!(self, CurrentState::Playing(_)) {
                    if let Err(err) = resources.device.stop() {
                        log::error!("failed to stop audio stream: {}", err);
                        resources
                            .broadcaster
                            .broadcast(PlayerMessage::EventAudioDeviceFailed(err.to_string()));
                    }
                    CurrentState::DoNothing
                } else {
                    self
                }
            }
            PlayerMessage::CommandLoadAndPlayLocation(location) => {
                log::info!("loading and playing location: {:?}", location);
                CurrentState::LoadLocation(StateLoadLocation { location })
            }
            _ => self,
        }
    }
}

impl State for CurrentState {
    fn update(self, resources: &mut PlayerThreadResources) -> Self {
        match self {
            CurrentState::DoNothing => self,
            CurrentState::Quit => self,
            CurrentState::LoadLocation(state) => state.update(resources),
            CurrentState::Playing(state) => state.update(resources),
            // The paused state is just holding onto the previous play state, so don't update it
            CurrentState::Paused(_) => self,
        }
    }
}

pub(super) struct StateManager {
    current: CurrentState,
}

impl StateManager {
    pub fn new() -> Self {
        Self {
            current: CurrentState::DoNothing,
        }
    }

    pub(super) fn should_quit(&self) -> bool {
        matches!(self.current, CurrentState::Quit)
    }

    pub(super) fn blocked_on_messages(&self) -> bool {
        match &self.current {
            CurrentState::Quit => false,
            CurrentState::DoNothing | CurrentState::Paused(_) => true,
            CurrentState::LoadLocation(_) | CurrentState::Playing(_) => false,
        }
    }

    pub(super) fn handle_message(
        &mut self,
        resources: &mut PlayerThreadResources,
        message: PlayerMessage,
    ) {
        let current = mem::take(&mut self.current);
        self.current = current.handle_message(resources, message);
    }

    pub(super) fn update(&mut self, resources: &mut PlayerThreadResources) {
        let current = mem::take(&mut self.current);
        self.current = current.update(resources);
    }
}

struct StatePlaying {
    source: AudioDecoderSource,
    status: PlaybackStatus,
    last_refresh_sent: Instant,
}

impl StatePlaying {
    fn new(source: AudioDecoderSource) -> Self {
        Self {
            source,
            status: PlaybackStatus {
                playing: true,
                position_secs: 0.0,
                duration_secs: None,
            },
            last_refresh_sent: Instant::now() - Duration::from_secs(2),
        }
    }

    fn transition_to_pause_state(mut self, resources: &PlayerThreadResources) -> CurrentState {
        log::info!("pausing playback");
        self.status.playing = false;
        resources
            .broadcaster
            .broadcast(PlayerMessage::UpdatePlaybackStatus(self.status));
        resources.device.pause().unwrap();
        CurrentState::Paused(self)
    }
}

impl State for StatePlaying {
    fn update(mut self, resources: &mut PlayerThreadResources) -> CurrentState {
        let maybe_next_state = queue_chunks(resources, &mut self.source);

        if let Some(waveform_calc) = resources.waveform_calculator.as_mut() {
            let mut waveform_lock = resources.waveform.lock().unwrap();
            if waveform_calc.waveform_needs_update(&waveform_lock) {
                waveform_calc.copy_latest_waveform_into(&mut *waveform_lock);
                drop(waveform_lock);
                resources
                    .broadcaster
                    .broadcast(PlayerMessage::UpdateWaveform(resources.waveform.clone()));
            }
        }

        let next_state = if let Some(new_state) = maybe_next_state {
            new_state
        } else {
            if Instant::now() - self.last_refresh_sent >= Duration::from_secs(1) {
                self.status.playing = true;

                let (frames_consumed, sample_rate) = (
                    resources.device.frames_consumed() as f64,
                    resources.device.playback_sample_rate() as f64,
                );
                self.status.position_secs = frames_consumed / sample_rate;

                let frame_count = self.source.frame_count();
                if self.status.duration_secs.is_none() && frame_count.is_some() {
                    self.status.duration_secs = frame_count.map(|fc| fc as f64 / sample_rate);
                }

                resources
                    .broadcaster
                    .broadcast(PlayerMessage::UpdatePlaybackStatus(self.status));
                self.last_refresh_sent = Instant::now();
            }
            CurrentState::Playing(self)
        };
        if let Some(sink) = resources.current_sink.as_ref() {
            sink.send_audio_with_timeout(Duration::from_millis(50));
        }
        next_state
    }
}

struct StateLoadLocation {
    location: Location,
}

impl State for StateLoadLocation {
    fn update(self, resources: &mut PlayerThreadResources) -> CurrentState {
        log::info!("loading location: {:?}", self.location);
        let preferred_format = PreferredFormat::new(
            resources.device.playback_sample_rate(),
            resources.device.playback_channels(),
        );
        let mut source = match AudioDecoderSource::new(self.location, preferred_format) {
            Ok(source) => source,
            Err(err) => {
                log::error!("failed to load location: {}", err);
                resources
                    .broadcaster
                    .broadcast(PlayerMessage::EventFailedToLoadLocation(err.into()));
                return CurrentState::DoNothing;
            }
        };
        if let Some(metadata) = source.metadata() {
            log::info!("loaded metaresources: {:?}", metadata);
            resources
                .broadcaster
                .broadcast(PlayerMessage::EventMetadataLoaded(metadata.clone()));
        }
        resources
            .device
            .pause()
            .expect("failed to pause audio stream");
        let state = if let Some(new_state) = queue_chunks(resources, &mut source) {
            new_state
        } else {
            resources
                .broadcaster
                .broadcast(PlayerMessage::EventStartedTrack);
            CurrentState::Playing(StatePlaying::new(source))
        };
        let device = &resources.device;
        device.reset_frames_consumed();
        device.play().expect("failed to pause audio stream");
        state
    }
}

fn queue_chunks(
    resources: &mut PlayerThreadResources,
    source: &mut AudioDecoderSource,
) -> Option<CurrentState> {
    while resources
        .current_sink
        .as_ref()
        .map(|s| s.needs_more_chunks())
        .unwrap_or(true)
    {
        match source.next_chunk() {
            Ok(Some(chunk)) => {
                if chunk.frame_count() > 0 {
                    let sample_rate = chunk.sample_rate();

                    // Note that since we're doing this during audio decode, there is a slight
                    // delay between the audio being played and the waveform being updated.
                    // However, this delay is small enough as to not be noticeable.
                    if resources.waveform_calculator.is_none() {
                        resources.waveform_calculator = Some(WaveformCalculator::new(sample_rate));
                    }
                    let waveform_calc = resources.waveform_calculator.as_mut().unwrap();
                    waveform_calc.push_source(&chunk);
                    waveform_calc.calculate();

                    let channels = chunk.channel_count();
                    let recreate_sink = match &resources.current_sink {
                        Some(sink) => {
                            sink.input_channels() != channels
                                || sink.input_sample_rate() != sample_rate
                        }
                        None => true,
                    };
                    if recreate_sink {
                        log::info!("recreating the audio sink");
                        if let Some(s) = resources.current_sink.as_ref() {
                            s.flush();
                        }
                        resources.current_sink =
                            Some(resources.device.create_sink(sample_rate, channels));
                    }
                    resources.current_sink.as_ref().unwrap().queue(chunk);
                }
            }
            Ok(None) => {
                log::info!("finished playing track");
                if let Some(sink) = resources.current_sink.as_ref() {
                    sink.flush();
                }
                resources.waveform_calculator = None;
                resources
                    .broadcaster
                    .broadcast(PlayerMessage::EventFinishedTrack);
                return Some(CurrentState::DoNothing);
            }
            Err(err) => {
                log::error!("error occurred while decoding audio: {}", err);
                resources
                    .broadcaster
                    .broadcast(PlayerMessage::EventFailedToDecodeAudio(err.into()));
                return Some(CurrentState::DoNothing);
            }
        }
    }
    None
}
