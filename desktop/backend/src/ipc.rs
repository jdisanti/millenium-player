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

use http::{Request, Response, StatusCode};
use millenium_desktop_assets::asset;
use millenium_post_office::{
    bytes::copy_f32s_into_ne_bytes,
    frontend::state::{PlaybackState, WaveformState},
};
use std::{borrow::Cow, mem::size_of};

pub struct InternalProtocol {
    playback_state: PlaybackState,
    waveform_state: WaveformState,
}

impl InternalProtocol {
    pub fn new(playback_state: PlaybackState, waveform_state: WaveformState) -> Self {
        Self {
            playback_state,
            waveform_state,
        }
    }

    pub fn handle_request(&self, request: Request<Vec<u8>>) -> http::Response<Cow<'static, [u8]>> {
        let path = request.uri().path().to_string();
        if path.starts_with("/ipc/") {
            self.handle_ipc_request(&path, request)
        } else {
            self.handle_asset_request(&path)
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
        request: Request<Vec<u8>>,
    ) -> Response<Cow<'static, [u8]>> {
        match path {
            "/ipc/playback" => self.handle_ipc_playback(request),
            "/ipc/waveform" => self.handle_ipc_waveform(request),
            _ => Self::error_not_found(),
        }
    }

    fn error_not_found() -> Response<Cow<'static, [u8]>> {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Cow::Borrowed(&b""[..]))
            .expect("valid response")
    }

    fn handle_ipc_playback(&self, _request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
        let state = self.playback_state.borrow();
        let body = serde_json::to_vec(&*state).expect("serializable");
        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(body.into())
            .expect("valid response")
    }

    fn handle_ipc_waveform(&self, _request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
        let state = self.waveform_state.borrow();
        if let Some(waves) = &state.waveform {
            let mut body = Vec::with_capacity(2 * waves.spectrum.len() * size_of::<f32>());
            copy_f32s_into_ne_bytes(&mut body, &waves.spectrum);
            copy_f32s_into_ne_bytes(&mut body, &waves.amplitude);
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/octet-stream")
                .body(body.into())
                .expect("valid response")
        } else {
            Self::error_not_found()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use millenium_post_office::{
        bytes::ne_bytes_to_f32s,
        frontend::state::{PlaybackStateData, Track, Waveform},
    };

    use super::*;

    #[test]
    fn asset_not_found() {
        let playback_state = PlaybackState::new();
        let waveform_state = WaveformState::new();
        let protocol = InternalProtocol::new(playback_state, waveform_state);

        let request = Request::builder()
            .uri("/does-not-exist")
            .method("GET")
            .body(Vec::new())
            .unwrap();
        let response = protocol.handle_request(request);
        assert_eq!(404, response.status());
        assert!(response.body().is_empty());
    }

    #[test]
    fn ipc_not_found() {
        let playback_state = PlaybackState::new();
        let waveform_state = WaveformState::new();
        let protocol = InternalProtocol::new(playback_state, waveform_state);

        let request = Request::builder()
            .uri("/ipc/does-not-exist")
            .method("GET")
            .body(Vec::new())
            .unwrap();
        let response = protocol.handle_request(request);
        assert_eq!(404, response.status());
        assert!(response.body().is_empty());
    }

    #[test]
    fn respond_with_asset() {
        let playback_state = PlaybackState::new();
        let waveform_state = WaveformState::new();
        let protocol = InternalProtocol::new(playback_state, waveform_state);

        let request = Request::builder()
            .uri("/static/test_asset.txt")
            .method("GET")
            .body(Vec::new())
            .unwrap();
        let response = protocol.handle_request(request);
        assert_eq!(200, response.status());
        assert_eq!(
            "text/plain",
            response.headers().get("content-type").unwrap()
        );
        assert_eq!(&b"test"[..], response.body().as_ref());
    }

    #[test]
    fn respond_with_playback_data() {
        let playback_state = PlaybackState::new();
        let waveform_state = WaveformState::new();
        let protocol = InternalProtocol::new(playback_state.clone(), waveform_state);

        playback_state.mutate(|state| {
            state.current_track = Some(Track {
                title: Some("test-title".into()),
                artist: Some("test-artist".into()),
                album: Some("test-album".into()),
            });
            state.playback_status.end_position = Some(Duration::from_secs(123));
            state.playback_status.current_position = Duration::from_secs(12);
        });

        let request = Request::builder()
            .uri("/ipc/playback")
            .method("GET")
            .body(Vec::new())
            .unwrap();
        let response = protocol.handle_request(request);
        assert_eq!(200, response.status());
        assert_eq!(
            "application/json",
            response.headers().get("content-type").unwrap()
        );

        let actual: PlaybackStateData = serde_json::from_slice(response.body()).unwrap();
        pretty_assertions::assert_eq!(*playback_state.borrow(), actual);
    }

    #[test]
    fn respond_with_waveform_data() {
        let playback_state = PlaybackState::new();
        let waveform_state = WaveformState::new();
        let protocol = InternalProtocol::new(playback_state, waveform_state.clone());

        waveform_state.mutate(|state| {
            state.waveform = Some(Waveform {
                spectrum: Box::new([1.0, 2.0, 3.0]),
                amplitude: Box::new([4.0, 5.0, 6.0]),
            })
        });

        let request = Request::builder()
            .uri("/ipc/waveform")
            .method("GET")
            .body(Vec::new())
            .unwrap();
        let response = protocol.handle_request(request);
        assert_eq!(200, response.status());
        assert_eq!(
            "application/octet-stream",
            response.headers().get("content-type").unwrap()
        );

        let body = response.body();
        let spectrum_bytes = &body[0..body.len() / 2];
        let amplitude_bytes = &body[body.len() / 2..];

        let spectrum = ne_bytes_to_f32s(spectrum_bytes);
        let amplitude = ne_bytes_to_f32s(amplitude_bytes);

        assert_eq!(&[1.0, 2.0, 3.0], &*spectrum);
        assert_eq!(&[4.0, 5.0, 6.0], &*amplitude);
    }
}
