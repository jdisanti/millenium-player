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

use crate::{error::FatalError, APP_TITLE};
use millenium_assets::asset;
use millenium_core::{
    location::Location,
    player::{
        message::{FromPlayerMessage, ToPlayerMessage},
        PlayerThread, PlayerThreadHandle,
    },
};
use std::{
    borrow::Cow,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

struct Playlist {
    locations: Vec<Location>,
    current: Option<usize>,
}

pub struct SimpleModeUi {
    /// MacOS has the special "always at the top" menu bar that needs to get populated.
    /// Menus aren't needed for the other OSes.
    #[cfg(target_os = "macos")]
    _osx_app_menu: OsxAppMenu,

    _main_web_view: wry::webview::WebView,
    event_loop: Option<tao::event_loop::EventLoop<()>>,

    player: Option<PlayerThreadHandle>,
    // TODO receive and handle messages from player thread
    _player_receiver: Receiver<FromPlayerMessage>,
    _playlist: Playlist,
}

impl SimpleModeUi {
    pub fn new(locations: &[Location]) -> Result<Self, FatalError> {
        let event_loop = tao::event_loop::EventLoop::new();
        let main_window = tao::window::WindowBuilder::new()
            .with_title(APP_TITLE)
            .with_decorations(true)
            .build(&event_loop)
            .map_err(|err| FatalError::new("failed to create window", err))?;
        let main_web_view = create_webview(main_window)?;

        let filtered_locations: Vec<Location> = locations
            .iter()
            .cloned()
            .filter(|location| !location.inferred_type().is_unknown())
            // TODO: remove the following filter and load playlists
            .filter(|location| !location.inferred_type().is_playlist())
            .collect();
        if filtered_locations.is_empty() && !locations.is_empty() {
            rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("Error")
                .set_description("None of the given files are audio or playlist files.")
                .show();
        }
        let (player_sender, player_receiver) = mpsc::channel();
        let player = PlayerThread::spawn(player_sender, None)?;
        let playlist = Playlist {
            current: filtered_locations.get(0).map(|_| Some(0)).unwrap_or(None),
            locations: filtered_locations,
        };
        if let Some(index) = playlist.current {
            player.send(ToPlayerMessage::LoadAndPlayLocation(
                playlist.locations[index].clone(),
            ))?;
        }

        Ok(Self {
            #[cfg(target_os = "macos")]
            _osx_app_menu: OsxAppMenu::new()?,

            _main_web_view: main_web_view,
            event_loop: Some(event_loop),

            player: Some(player),
            _player_receiver: player_receiver,
            _playlist: playlist,
        })
    }

    pub fn run(mut self) -> ! {
        use tao::event::{Event, WindowEvent};
        use tao::event_loop::ControlFlow;

        let event_loop = self.event_loop.take().expect("event loop");
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100));

            match event {
                Event::LoopDestroyed => {
                    if let Some(player) = self.player.take() {
                        let _ = player.send(ToPlayerMessage::Quit);
                        if let Err(err) = player.join() {
                            log::error!("{err}");
                        }
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => *control_flow = ControlFlow::Exit,

                _ => (),
            }

            if let Err(err) = self.healthcheck() {
                log::error!("{err}");
                rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Error)
                    .set_title("Fatal error")
                    .set_description(&format!("{APP_TITLE} had a fatal error:\n{err}"))
                    .show();
                *control_flow = ControlFlow::ExitWithCode(1);
            }
        });
    }

    fn healthcheck(&mut self) -> Result<(), FatalError> {
        if let Some(player) = self.player.take() {
            match player.healthcheck() {
                Ok(player) => self.player = Some(player),
                Err(err) => return Err(err.into()),
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
struct OsxAppMenu {
    _menu: muda::Menu,
}

#[cfg(target_os = "macos")]
impl OsxAppMenu {
    fn new() -> Result<Self, FatalError> {
        use muda::{AboutMetadata, Menu, PredefinedMenuItem, Submenu};

        let menu = Menu::new();

        let app_menu = Submenu::new(APP_TITLE, true);
        app_menu
            .append_items(&[
                &PredefinedMenuItem::about(None, Some(AboutMetadata::default())),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ])
            .unwrap();

        let window_menu = Submenu::new("Window", true);
        window_menu
            .append(&PredefinedMenuItem::minimize(None))
            .unwrap();

        menu.append_items(&[&app_menu, &window_menu]).unwrap();

        menu.init_for_nsapp();
        window_menu.set_as_windows_menu_for_nsapp();
        Ok(Self { _menu: menu })
    }
}

fn create_webview(window: tao::window::Window) -> Result<wry::webview::WebView, FatalError> {
    let webview = wry::webview::WebViewBuilder::new(window)
        .map_err(|err| FatalError::new("failed to create web view", err))?
        .with_custom_protocol("internal".into(), internal_asset)
        .with_ipc_handler(|_window, message| {
            dbg!(message);
        })
        .with_url("internal://localhost/simple_mode.html")
        .map_err(|err| FatalError::new("failed to set web view URL", err))?
        .with_file_drop_handler(|_window, event| {
            dbg!(event);
            true
        })
        .with_transparent(true)
        .with_visible(false)
        .build()
        .map_err(|err| FatalError::new("failed to create web view", err))?;
    Ok(webview)
}

fn internal_asset(
    request: &http::Request<Vec<u8>>,
) -> Result<http::Response<Cow<'static, [u8]>>, wry::Error> {
    let path = request.uri().path();
    log::info!("loading asset \"{path}\"");
    match asset(&path[1..]) {
        Ok(asset) => Ok(http::Response::builder()
            .status(200)
            .body(Cow::Owned(asset))
            .unwrap()),
        Err(err) => {
            log::error!("{err}");
            Ok(http::Response::builder()
                .status(404)
                .body(Cow::Owned(format!("{err}").into_bytes()))
                .unwrap())
        }
    }
}
