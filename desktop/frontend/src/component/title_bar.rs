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
use millenium_post_office::frontend::message::FrontendMessage;
use yew::prelude::*;

#[function_component(TitleBar)]
pub fn title_bar() -> Html {
    let drag = |_| post_message(&FrontendMessage::DragWindowStart);
    let close = |_| post_message(&FrontendMessage::Quit);
    html! {
        <div class="title-bar">
            <div class="button-bar">
                <button type="button" class="close" aria-label="close" onclick={close}><i></i></button>
                <button type="button" class="minimize" disabled={true}></button>
                <button type="button" class="maximize" disabled={true}></button>
            </div>
            <div class="title-bar-text" onmousedown={drag}>{ "Millenium Player" }</div>
            <div class="third-bar"></div>
        </div>
    }
}
