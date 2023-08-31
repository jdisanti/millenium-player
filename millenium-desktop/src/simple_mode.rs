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
        waveform::Waveform,
        PlayerThread, PlayerThreadHandle,
    },
};
use std::{
    borrow::Cow,
    sync::{
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tao::{
    dpi::{LogicalSize, Size},
    event_loop::{EventLoop, EventLoopBuilder, EventLoopProxy},
};
use wry::webview::webview_version;

struct Playlist {
    locations: Vec<Location>,
    current: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
struct UiData {
    waveform: Waveform,
}

pub struct SimpleModeUi {
    /// MacOS has the special "always at the top" menu bar that needs to get populated.
    /// Menus aren't needed for the other OSes.
    #[cfg(target_os = "macos")]
    _osx_app_menu: OsxAppMenu,

    main_web_view: wry::webview::WebView,
    event_loop: Option<tao::event_loop::EventLoop<FromUiEvent>>,

    player: Option<PlayerThreadHandle>,
    player_receiver: Receiver<FromPlayerMessage>,
    _playlist: Playlist,

    ui_data: Arc<Mutex<UiData>>,
}

impl SimpleModeUi {
    pub fn new(locations: &[Location]) -> Result<Self, FatalError> {
        let ui_data = Arc::new(Mutex::new(UiData {
            waveform: Waveform::empty(),
        }));

        let event_loop: EventLoop<FromUiEvent> = EventLoopBuilder::with_user_event().build();
        let main_window = tao::window::WindowBuilder::new()
            .with_title(APP_TITLE)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_inner_size(Size::Logical(LogicalSize::new(400.0, 200.0)))
            .with_visible(false) // start invisible
            .build(&event_loop)
            .map_err(|err| FatalError::new("failed to create window", err))?;
        let main_web_view =
            create_webview(main_window, event_loop.create_proxy(), ui_data.clone())?;

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

            main_web_view,
            event_loop: Some(event_loop),

            player: Some(player),
            player_receiver,
            _playlist: playlist,

            ui_data,
        })
    }

    pub fn run(mut self) -> ! {
        use tao::event::{Event, WindowEvent};
        use tao::event_loop::ControlFlow;

        log::info!("starting event loop");
        let mut start_time = Some(Instant::now());

        let event_loop = self.event_loop.take().expect("event loop");
        event_loop.run(move |event, _, control_flow| {
            // Show the window after 150 milliseconds to avoid the flashing white window on startup
            if start_time.is_some()
                && Instant::now() - start_time.unwrap() > Duration::from_millis(150)
            {
                log::info!("showing main window");
                self.main_web_view.window().set_visible(true);
                start_time = None;
            }
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
                Event::UserEvent(FromUiEvent::Quit)
                | Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => *control_flow = ControlFlow::Exit,

                Event::UserEvent(FromUiEvent::DragWindowStart) => {
                    self.main_web_view.window().drag_window().unwrap()
                }

                _ => (),
            }

            if let Ok(message) = self.player_receiver.try_recv() {
                match message {
                    FromPlayerMessage::AudioDeviceCreationFailed(_err) => {
                        // TODO
                    }
                    FromPlayerMessage::AudioDeviceFailed(_err) => {
                        // TODO
                    }
                    FromPlayerMessage::FailedToDecodeAudio(_err) => {
                        // TODO
                    }
                    FromPlayerMessage::FailedToLoadLocation(_err) => {
                        // TODO
                    }
                    FromPlayerMessage::FinishedTrack => {
                        let mut ui_data_lock = self.ui_data.lock().unwrap();
                        ui_data_lock.waveform.copy_from(&Waveform::empty());
                    }
                    FromPlayerMessage::MetadataLoaded(_metadata) => {
                        // TODO
                    }
                    FromPlayerMessage::Waveform(waveform) => {
                        let waveform_lock = waveform.lock().unwrap();
                        let mut ui_data_lock = self.ui_data.lock().unwrap();
                        ui_data_lock.waveform.copy_from(&*waveform_lock);
                    }
                }
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

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "kind")]
enum FromUiEvent {
    Quit,
    DragWindowStart,
}

#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind")]
enum ToUiEvent {
    // ...
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

fn create_webview(
    window: tao::window::Window,
    event_loop_proxy: EventLoopProxy<FromUiEvent>,
    ui_data: Arc<Mutex<UiData>>,
) -> Result<wry::webview::WebView, FatalError> {
    log::info!(
        "webview version: {}",
        webview_version().as_deref().unwrap_or("unknown")
    );
    let webview = wry::webview::WebViewBuilder::new(window)
        .map_err(|err| FatalError::new("failed to create web view", err))?
        .with_hotkeys_zoom(false)
        .with_download_started_handler(|_,_| false)  // don't allow file downloads
        .with_custom_protocol("internal".into(), {
            let ui_data = ui_data.clone();
            move |request| internal_protocol(ui_data.clone(), request)
        })
        .with_ipc_handler(move |_window, message| {
            match serde_json::from_str::<FromUiEvent>(&message) {
                Ok(event) => event_loop_proxy.send_event(event).unwrap(),
                Err(err) => {
                    log::error!("failed to deserialize IPC message from the webview: {err}\nmessage: {message}")
                }
            }
        })
        .with_url("internal://localhost/simple_mode.html")
        .map_err(|err| FatalError::new("failed to set web view URL", err))?
        .with_file_drop_handler(|_window, event| {
            // TODO: handle file drop by changing playing media
            dbg!(event);
            true
        })
        .with_transparent(true)
        .build()
        .map_err(|err| FatalError::new("failed to create web view", err))?;
    Ok(webview)
}

fn internal_protocol(
    ui_data: Arc<Mutex<UiData>>,
    request: &http::Request<Vec<u8>>,
) -> Result<http::Response<Cow<'static, [u8]>>, wry::Error> {
    fn response_404() -> Result<http::Response<Cow<'static, [u8]>>, wry::Error> {
        let empty = Cow::Borrowed(&[][..]);
        Ok(http::Response::builder().status(404).body(empty).unwrap())
    }

    let path = request.uri().path();
    if path.starts_with("/ipc/") {
        match path {
            "/ipc/ui-data" => {
                let ui_data_lock = ui_data.lock().unwrap();
                let body = serde_json::to_vec(&*ui_data_lock).unwrap();
                Ok(http::Response::builder()
                    .status(200)
                    .header("Content-Type", "application/json")
                    .body(body.into())
                    .unwrap())
            }
            _ => {
                log::error!("unknown IPC request: {path}");
                response_404()
            }
        }
    } else {
        log::info!("loading asset \"{path}\"");
        match asset(&path[1..]) {
            Ok(asset) => Ok(http::Response::builder()
                .status(200)
                .header("Content-Type", asset.mime)
                .body(asset.contents)
                .unwrap()),
            Err(err) => {
                log::error!("{err}");
                response_404()
            }
        }
    }
}
