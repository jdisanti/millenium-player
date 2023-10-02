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

use crate::message::post_message;
use millenium_post_office::{frontend::message::FrontendMessage, types::Volume};
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct VolumeSliderProps {
    pub volume: Volume,
}

#[function_component(VolumeSlider)]
pub fn volume_slider(props: &VolumeSliderProps) -> Html {
    let oninput = |event: InputEvent| {
        let value = input_value!(event);
        if let Ok(volume) = value.parse::<u8>() {
            post_message(&FrontendMessage::MediaControlVolume {
                volume: Volume::new(volume),
            });
        }
    };
    let min = u8::from(Volume::min()).to_string();
    let max = u8::from(Volume::max()).to_string();
    html! {
        <div class="volume-slider">
            <i></i>
            <input type="range" step="1" min={min} max={max} value={u8::from(props.volume).to_string()} oninput={oninput} />
        </div>
    }
}
