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

use crate::ui::{SharedUiResources, UiResources};
use http::{Request, Response, StatusCode};
use millenium_core::player::message::PlaybackStatus;
use millenium_desktop_assets::asset;
use std::{borrow::Cow, mem::size_of};

pub struct InternalProtocol {
    resources: SharedUiResources,
}

impl InternalProtocol {
    pub fn new(resources: SharedUiResources) -> Self {
        Self { resources }
    }

    pub fn handle_request(&self, request: &Request<Vec<u8>>) -> http::Response<Cow<'static, [u8]>> {
        let path = request.uri().path();
        if path.starts_with("/ipc/") {
            self.handle_ipc_request(path, request)
        } else {
            self.handle_asset_request(path)
        }
    }

    fn handle_asset_request(&self, path: &str) -> http::Response<Cow<'static, [u8]>> {
        log::info!("loading asset \"{path}\"");
        match asset(&path[1..]) {
            Ok(asset) => Response::builder()
                .status(200)
                .header("Content-Type", asset.mime)
                .body(asset.contents)
                .unwrap(),
            Err(err) => {
                log::error!("{err}");
                Self::error_not_found()
            }
        }
    }

    fn handle_ipc_request(
        &self,
        path: &str,
        request: &Request<Vec<u8>>,
    ) -> Response<Cow<'static, [u8]>> {
        match path {
            "/ipc/playing-data" => self.handle_ipc_playing_data(request),
            "/ipc/waveform-data" => self.handle_ipc_waveform_data(request),
            _ => Self::error_not_found(),
        }
    }

    fn error_not_found() -> Response<Cow<'static, [u8]>> {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Cow::Borrowed(&b""[..]))
            .expect("valid response")
    }

    fn handle_ipc_playing_data(&self, _request: &Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
        let resources = self.resources.borrow();
        let playing = Playing::from(&*resources);
        let body = serde_json::to_vec(&playing).expect("serializable");
        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(body.into())
            .expect("valid response")
    }

    fn handle_ipc_waveform_data(
        &self,
        _request: &Request<Vec<u8>>,
    ) -> Response<Cow<'static, [u8]>> {
        let resources = self.resources.borrow();
        let waves = &resources.waveform;
        let mut body = Vec::with_capacity(2 * waves.spectrum.len() * size_of::<f32>());
        copy_f32s_into_ne_bytes(&mut body, &waves.spectrum);
        copy_f32s_into_ne_bytes(&mut body, &waves.amplitude);
        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/octet-stream")
            .body(body.into())
            .expect("valid response")
    }
}

fn copy_f32s_into_ne_bytes(into: &mut Vec<u8>, data: &[f32]) {
    for &value in data {
        into.extend_from_slice(&value.to_ne_bytes()[..]);
    }
}

#[derive(Default, Debug, serde::Serialize)]
pub struct Playing<'a> {
    pub title: Option<&'a str>,
    pub artist: Option<&'a str>,
    pub album: Option<&'a str>,
    pub status: PlaybackStatus,
}

impl Playing<'static> {
    pub fn empty() -> Self {
        Self {
            title: None,
            artist: None,
            album: None,
            status: PlaybackStatus::default(),
        }
    }
}

impl<'a> From<&'a UiResources> for Playing<'a> {
    fn from(resources: &'a UiResources) -> Self {
        if let Some(metadata) = &resources.metadata {
            Playing {
                title: metadata.track_title.as_deref(),
                artist: metadata.artist.as_deref(),
                album: metadata.album.as_deref(),
                status: resources.playback_status,
            }
        } else {
            Playing {
                status: resources.playback_status,
                ..Self::default()
            }
        }
    }
}
