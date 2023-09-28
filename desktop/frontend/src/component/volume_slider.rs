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

use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct VolumeSliderProps {
    pub volume: u8,
}

#[function_component(VolumeSlider)]
pub fn volume_slider(props: &VolumeSliderProps) -> Html {
    html! {
        <div class="volume-slider">
            <i></i>
            <input type="range" min="0" max="100" value={props.volume.to_string()} />
        </div>
    }
}
