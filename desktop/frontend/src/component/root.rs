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

use crate::component::{
    media_controls::MediaControls, media_info::MediaInfo, title_bar::TitleBar, waveform::Waveform,
};
use millenium_post_office::frontend::state::{PlaybackStateData, WaveformStateData};
use once_cell::sync::Lazy;
use std::{cell::RefCell, rc::Rc};
use yew::prelude::*;

static EMPTY_PLAYBACK_STATE: Lazy<PlaybackStateData> = Lazy::new(PlaybackStateData::default);

pub enum RootMessage {
    UpdatePlaybackState(Rc<PlaybackStateData>),
    UpdateWaveformState(WaveformStateData),
}

#[derive(Default, Properties, PartialEq)]
pub struct RootProps {}

#[derive(Default)]
pub struct Root {
    playback_state: Option<Rc<PlaybackStateData>>,
    waveform_state: Option<Rc<RefCell<WaveformStateData>>>,
}

impl Component for Root {
    type Message = RootMessage;
    type Properties = RootProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Default::default()
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            RootMessage::UpdatePlaybackState(state) => {
                self.playback_state = Some(state);
                true
            }
            RootMessage::UpdateWaveformState(state) => {
                if let Some(waveform_state) = self.waveform_state.as_mut() {
                    *waveform_state.borrow_mut() = state;
                    false
                } else {
                    self.waveform_state = Some(Rc::new(RefCell::new(state)));
                    true
                }
            }
        }
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        let state = self
            .playback_state
            .as_deref()
            .unwrap_or(&EMPTY_PLAYBACK_STATE);
        let playing = state.playback_status.playing;

        let waveform = self
            .waveform_state
            .as_ref()
            .map(|w| html!(<Waveform waveform={w} />))
            .unwrap_or_else(|| html!(<div class="waveform-placeholder" />));
        let media_info = self
            .playback_state
            .as_ref()
            .map(|s| html!(<MediaInfo state={s} />));

        html! {
            <>
                {waveform}
                <div class="window simple-mode">
                    <TitleBar />
                    <div style="padding:10px;">
                        {media_info}
                        <MediaControls playing={playing}
                                       playlist_mode={state.playlist_mode}
                                       volume={state.playback_status.volume} />
                    </div>
                </div>
            </>
        }
    }
}
