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

use crate::component::root::{Root, RootMessage};
use gloo::net::http::Request;
use millenium_post_office::{
    bytes::ne_bytes_to_f32s,
    frontend::{
        message::FrontendMessage,
        state::{PlaybackStateData, Waveform, WaveformStateData},
    },
};
use std::rc::Rc;
use yew::{platform::spawn_local, AppHandle};

mod component {
    pub mod duration;
    pub mod media_controls;
    pub mod media_info;
    pub mod root;
    pub mod title_bar;
    pub mod volume_slider;
    pub mod waveform;
}
mod log;
mod message;

static mut ROOT_HANDLE: Option<AppHandle<Root>> = None;
fn root_handle_mut() -> &'static mut AppHandle<Root> {
    // Safe because there isn't any multi-threading in the frontend
    unsafe {
        ROOT_HANDLE
            .as_mut()
            .expect("root_handle must be initialized by now")
    }
}
fn set_root_handle(root_handle: AppHandle<Root>) {
    // Safe because there isn't any multi-threading in the frontend
    unsafe { ROOT_HANDLE = Some(root_handle) }
}

fn main() {
    info!("frontend started");

    let body = gloo::utils::document()
        .body()
        .expect("no body element found");
    let root = body
        .query_selector("#root-content")
        .expect("failed to query DOM")
        .expect("failed to find the #root-content element");
    set_root_handle(yew::Renderer::<component::root::Root>::with_root(root).render());
}

fn handle_message(message: FrontendMessage) {
    match message {
        FrontendMessage::PlaybackStateUpdated => spawn_local(fetch_playback_data()),
        FrontendMessage::WaveformStateUpdated => spawn_local(fetch_waveform_data()),
        _ => {}
    }
}

async fn fetch_playback_data() {
    let response = Request::get("/ipc/playback").send().await;
    match response {
        Ok(response) => {
            let data = match response.json::<PlaybackStateData>().await {
                Ok(data) => data,
                Err(err) => {
                    error!("failed to parse playback state: {err}");
                    return;
                }
            };
            root_handle_mut().send_message(RootMessage::UpdatePlaybackState(Rc::new(data)));
        }
        Err(err) => {
            error!("failed to fetch playback state: {err}");
        }
    }
}

async fn fetch_waveform_data() {
    let response = Request::get("/ipc/waveform").send().await;
    match response {
        Ok(response) => {
            let bytes = match response.binary().await {
                Ok(bytes) => bytes,
                Err(err) => {
                    error!("failed to load waveform response body: {err}");
                    return;
                }
            };
            let (spectrum_bytes, amplitude_bytes) = bytes.split_at(bytes.len() / 2);
            let spectrum = ne_bytes_to_f32s(spectrum_bytes);
            let amplitude = ne_bytes_to_f32s(amplitude_bytes);

            root_handle_mut().send_message(RootMessage::UpdateWaveformState(WaveformStateData {
                waveform: Some(Waveform {
                    spectrum,
                    amplitude,
                }),
            }));
        }
        Err(err) => {
            error!("failed to fetch waveform state: {err}");
        }
    }
}
