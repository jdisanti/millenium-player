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

use millenium_post_office::frontend::state::PlaybackStateData;
use std::rc::Rc;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct MediaInfoProps {
    pub state: Rc<PlaybackStateData>,
}

#[function_component(MediaInfo)]
pub fn media_info(props: &MediaInfoProps) -> Html {
    if let Some(track) = props.state.current_track.as_ref() {
        let artist = track.artist.as_deref().unwrap_or("Unknown artist");
        let title = track.title.as_deref().unwrap_or("Untitled");
        let album = track.album.as_deref().unwrap_or("Unknown album");
        html! {
            <>
                <p>{artist}{" - "}{title}</p>
                <p>{album}</p>
            </>
        }
    } else {
        html!()
    }
}
