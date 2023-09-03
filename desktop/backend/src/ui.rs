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

use crate::{
    args::Mode,
    error::FatalError,
    ipc::{
        from_ui::{FromUiMessage, MessageHandler},
        to_ui::InternalProtocol,
    },
    APP_TITLE,
};
use camino::Utf8Path;
use millenium_core::{
    location::Location,
    metadata::Metadata,
    player::{
        message::{FromPlayerMessage, PlaybackStatus, ToPlayerMessage},
        waveform::Waveform,
        PlayerThread, PlayerThreadHandle,
    },
};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
use tao::{
    dpi::{LogicalSize, Size},
    event_loop::{EventLoop, EventLoopBuilder, EventLoopProxy},
};
use wry::webview::{webview_version, FileDropEvent};

struct Playlist {
    locations: Vec<Location>,
    current: Option<usize>,
}

pub struct UiResources {
    player: Option<PlayerThreadHandle>,
    pub waveform: Waveform,
    pub metadata: Option<Metadata>,
    pub playback_status: PlaybackStatus,
}

impl UiResources {
    pub fn new() -> Self {
        Self {
            player: None,
            waveform: Waveform::empty(),
            metadata: None,
            playback_status: PlaybackStatus::default(),
        }
    }

    pub fn player(&self) -> &PlayerThreadHandle {
        self.player.as_ref().expect("player should be set by now")
    }
}

impl Default for UiResources {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedUiResources = Rc<RefCell<UiResources>>;

pub struct Ui {
    /// MacOS has the special "always at the top" menu bar that needs to get populated.
    /// Menus aren't needed for the other OSes.
    #[cfg(target_os = "macos")]
    _osx_app_menu: OsxAppMenu,

    main_web_view: wry::webview::WebView,
    event_loop: Option<tao::event_loop::EventLoop<FromUiMessage>>,

    player: Option<PlayerThreadHandle>,
    player_receiver: Receiver<FromPlayerMessage>,
    _playlist: Playlist,

    message_handler: MessageHandler,
    resources: SharedUiResources,
}

impl Ui {
    pub fn new(mode: Mode) -> Result<Self, FatalError> {
        let resources = Rc::new(RefCell::new(UiResources::new()));
        let to_ui = Rc::new(InternalProtocol::new(resources.clone()));

        let event_loop: EventLoop<FromUiMessage> = EventLoopBuilder::with_user_event().build();
        let main_window = tao::window::WindowBuilder::new()
            .with_title(APP_TITLE)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_inner_size(Size::Logical(LogicalSize::new(400.0, 200.0)))
            .with_visible(false) // start invisible
            .build(&event_loop)
            .map_err(|err| FatalError::new("failed to create window", err))?;
        let main_web_view = create_webview(main_window, event_loop.create_proxy(), to_ui)?;

        let playlist = match mode {
            Mode::Simple { locations } => {
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
                Playlist {
                    current: filtered_locations.get(0).map(|_| Some(0)).unwrap_or(None),
                    locations: filtered_locations,
                }
            }
            Mode::Library {
                storage_path,
                audio_path,
            } => {
                let _ = (storage_path, audio_path);
                unimplemented!("library mode isn't implemented yet")
            }
        };

        let (player_sender, player_receiver) = mpsc::channel();
        let player = PlayerThread::spawn(player_sender, None)?;
        if let Some(index) = playlist.current {
            player.send(ToPlayerMessage::LoadAndPlayLocation(
                playlist.locations[index].clone(),
            ))?;
        }
        resources.borrow_mut().player = Some(player.weak_clone());

        Ok(Self {
            #[cfg(target_os = "macos")]
            _osx_app_menu: OsxAppMenu::new()?,

            main_web_view,
            event_loop: Some(event_loop),

            player: Some(player),
            player_receiver,
            _playlist: playlist,

            message_handler: MessageHandler::new(resources.clone()),
            resources,
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
            *control_flow =
                ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(1000 / 60));

            match event {
                Event::LoopDestroyed => {
                    if let Some(player) = self.player.take() {
                        let _ = player.send(ToPlayerMessage::Quit);
                        if let Err(err) = player.join() {
                            log::error!("{err}");
                        }
                    }
                    log::info!("bye!");
                }
                Event::UserEvent(FromUiMessage::Quit)
                | Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => *control_flow = ControlFlow::Exit,

                Event::UserEvent(FromUiMessage::DragWindowStart) => {
                    self.main_web_view.window().drag_window().unwrap()
                }

                Event::UserEvent(message) => {
                    self.message_handler.handle(message);
                }

                _ => (),
            }

            self.handle_player_messages();

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

    fn handle_player_messages(&self) {
        if let Ok(message) = self.player_receiver.try_recv() {
            let mut resources = self.resources.borrow_mut();
            let frequent_message = matches!(&message, &FromPlayerMessage::Waveform(_));
            if !frequent_message {
                log::info!("received message from player: {message:?}");
            }
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
                FromPlayerMessage::PlaybackStatus(status) => {
                    resources.playback_status = status;
                }
                FromPlayerMessage::StartedTrack => {}
                FromPlayerMessage::FinishedTrack => {
                    resources.waveform.copy_from(&Waveform::empty());
                    resources.playback_status = PlaybackStatus::default();
                    resources.metadata = None;
                }
                FromPlayerMessage::MetadataLoaded(metadata) => {
                    resources.metadata = Some(metadata);
                }
                FromPlayerMessage::Waveform(waveform) => {
                    let waveform_lock = waveform.lock().unwrap();
                    resources.waveform.copy_from(&waveform_lock);
                }
            }

            let _ = resources;
            if !frequent_message {
                self.main_web_view
                    .evaluate_script(r#"millenium.Message.handle("state_updated", null)"#)
                    .expect("valid script");
            }
        }
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

fn create_webview(
    window: tao::window::Window,
    event_loop_proxy: EventLoopProxy<FromUiMessage>,
    to_ui: Rc<InternalProtocol>,
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
            move |request| {
                let to_ui = to_ui.clone();
                Ok(to_ui.handle_request(request))
            }
        })
        .with_ipc_handler({
            let proxy = event_loop_proxy.clone();
            move |_window, message| {
                match serde_json::from_str::<FromUiMessage>(&message) {
                    Ok(event) => proxy.send_event(event).unwrap(),
                    Err(err) => {
                        log::error!("failed to deserialize IPC message from the webview: {err}\nmessage: {message}")
                    }
                }
            }
        })
        .with_url("internal://localhost/index.html")
        .map_err(|err| FatalError::new("failed to set web view URL", err))?
        .with_file_drop_handler(move |_window, event| {
            if let FileDropEvent::Dropped { paths, .. } = event {
                let locations = paths.into_iter()
                    .map(|path| Utf8Path::from_path(&path).unwrap().to_string())
                    .collect::<Vec<_>>();
                event_loop_proxy.send_event(FromUiMessage::LoadLocations { locations }).unwrap();
            }
            true
        })
        .with_transparent(true)
        .build()
        .map_err(|err| FatalError::new("failed to create web view", err))?;
    Ok(webview)
}
