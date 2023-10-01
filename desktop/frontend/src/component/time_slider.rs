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

use crate::{component::duration::Duration as DurationComponent, message::post_message};
use millenium_post_office::frontend::message::FrontendMessage;
use std::time::Duration;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct TimeSliderProps {
    pub current_position: Duration,
    /// End position in the audio track (length of the track). If `None`, then we are streaming audio.
    pub end_position: Option<Duration>,
}

#[function_component(TimeSlider)]
pub fn time_slider(props: &TimeSliderProps) -> Html {
    let (prefix, input, suffix) = if let Some(length) = props.end_position {
        let onchange = |event: Event| {
            let value = input_value!(event);
            let secs = value.parse::<u64>().expect("valid integer");
            let position = Duration::from_secs(secs);
            post_message(&FrontendMessage::MediaControlSeek { position });
        };
        let value = props.current_position.as_secs().to_string();
        let max = length.as_secs().to_string();
        (
            html! { <DurationComponent duration={props.current_position} /> },
            html! { <input type="range" step="1" min="0" max={max} value={value} onchange={onchange} /> },
            html! { <DurationComponent duration={length} /> },
        )
    } else {
        let zero = Duration::from_secs(0);
        (
            html! { <DurationComponent duration={zero} /> },
            html! { <input type="range" min="0" max="0" value="0" disabled={true} /> },
            html! { <DurationComponent duration={zero} /> },
        )
    };

    html! {
        <div class="time-slider">
            <div class="time-slider-duration"><span>{prefix}</span></div>
            <div class="time-slider-input">{input}</div>
            <div class="time-slider-duration"><span>{suffix}</span></div>
        </div>
    }
}
