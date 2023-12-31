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

use crate::{args::Mode, error::FatalError, ipc::InternalProtocol, APP_TITLE};
use camino::Utf8Path;
use millenium_core::{
    location::Location,
    message::{PlayerMessage, PlayerMessageChannel},
    player::{PlayerThread, PlayerThreadHandle},
    playlist::PlaylistManager,
};
use millenium_post_office::{
    broadcast::{BroadcastMessage, BroadcastSubscription, Broadcaster, NoChannels},
    frontend::{
        message::{AlertLevel, FrontendMessage, LogLevel},
        state::{PlaybackState, PlaybackStatus, Track, Waveform, WaveformState},
    },
    state::StateChanged,
};
use muda::{ContextMenu, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};
use tao::{
    dpi::{LogicalSize, Size},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder},
    window::Window,
};
use wry::webview::{webview_version, FileDropEvent};

struct MediaControlsMenu {
    menu: Menu,
    item_open: MenuItem,
    item_show_hide_playlist: MenuItem,
}

impl MediaControlsMenu {
    fn new() -> Self {
        let menu = Menu::new();
        let item_open = MenuItem::new("Open", true, None);
        let item_show_hide_playlist = MenuItem::new("Show/hide playlist", true, None);
        menu.append_items(&[
            &item_open,
            &PredefinedMenuItem::separator(),
            &item_show_hide_playlist,
        ])
        .unwrap();
        Self {
            menu,
            item_open,
            item_show_hide_playlist,
        }
    }

    fn show(&self, window: &Window) {
        #[cfg(target_os = "windows")]
        {
            use tao::platform::windows::WindowExtWindows;
            self.menu
                .show_context_menu_for_hwnd(window.hwnd() as _, None);
        }
        #[cfg(target_os = "macos")]
        {
            use tao::platform::macos::WindowExtMacOS;
            self.menu
                .show_context_menu_for_nsview(window.ns_view() as _, None);
        }
        #[cfg(target_os = "linux")]
        {
            use tao::platform::unix::WindowExtUnix;
            self.menu
                .show_context_menu_for_gtk_window(window.gtk_window() as _, None);
        }
    }
}

pub struct Ui {
    /// MacOS has the special "always at the top" menu bar that needs to get populated.
    /// Menus aren't needed for the other OSes.
    #[cfg(target_os = "macos")]
    _osx_app_menu: OsxAppMenu,

    main_web_view: wry::webview::WebView,
    event_loop: Option<tao::event_loop::EventLoop<()>>,

    player: Option<PlayerThreadHandle>,
    player_sub: BroadcastSubscription<PlayerMessage>,
    _frontend_broadcaster: Broadcaster<FrontendMessage>,
    frontend_sub: BroadcastSubscription<FrontendMessage>,
    playlist_manager: PlaylistManager,

    playback_state: PlaybackState,
    playback_state_sub: BroadcastSubscription<StateChanged>,
    waveform_state: WaveformState,
    waveform_state_sub: BroadcastSubscription<StateChanged>,

    media_controls_menu: MediaControlsMenu,
}

impl Ui {
    pub fn new(mode: Mode) -> Result<Self, FatalError> {
        let playback_state = PlaybackState::new();
        let playback_state_sub = playback_state.subscribe("backend");
        let waveform_state = WaveformState::new();
        let waveform_state_sub = waveform_state.subscribe("backend");
        let protocol = Rc::new(InternalProtocol::new(
            playback_state.clone(),
            waveform_state.clone(),
        ));

        let frontend_broadcaster = Broadcaster::new();
        let frontend_sub = frontend_broadcaster.subscribe("backend", NoChannels);

        let event_loop: EventLoop<()> = EventLoopBuilder::new().build();
        let main_window = tao::window::WindowBuilder::new()
            .with_title(APP_TITLE)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_inner_size(Size::Logical(LogicalSize::new(400.0, 200.0)))
            .with_visible(false) // start invisible
            .build(&event_loop)
            .map_err(|err| FatalError::new("failed to create window", err))?;
        let main_web_view = create_webview(main_window, frontend_broadcaster.clone(), protocol)?;

        let player = PlayerThread::spawn(None)?;
        let player_sub = player.broadcaster().subscribe(
            "ui-backend",
            PlayerMessageChannel::Events | PlayerMessageChannel::FrequentUpdates,
        );

        let playlist_manager =
            PlaylistManager::new(player.broadcaster().clone(), frontend_broadcaster.clone());
        match mode {
            Mode::Simple { locations } => frontend_sub.broadcast(FrontendMessage::LoadLocations {
                locations: locations.iter().map(Location::to_string).collect(),
            }),
            Mode::Library {
                storage_path,
                audio_path,
            } => {
                let _ = (storage_path, audio_path);
                unimplemented!("library mode isn't implemented yet")
            }
        }

        Ok(Self {
            #[cfg(target_os = "macos")]
            _osx_app_menu: OsxAppMenu::new()?,

            main_web_view,
            event_loop: Some(event_loop),

            player: Some(player),
            player_sub,
            _frontend_broadcaster: frontend_broadcaster,
            frontend_sub,
            playlist_manager,

            playback_state,
            playback_state_sub,
            waveform_state,
            waveform_state_sub,

            media_controls_menu: MediaControlsMenu::new(),
        })
    }

    pub fn run(mut self) -> ! {
        use tao::event::{Event, WindowEvent};

        log::info!("starting event loop");
        let mut start_time = Some(Instant::now());

        let menu_event_receiver = MenuEvent::receiver();
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

            self.handle_player_messages();
            if let Some(new_flow) = self.handle_frontend_messages() {
                *control_flow = new_flow;
            }
            self.playlist_manager.update();

            if let Some(StateChanged) = self.playback_state_sub.try_recv() {
                let message = serde_json::to_string(&FrontendMessage::PlaybackStateUpdated)
                    .expect("serializable");
                self.main_web_view
                    .evaluate_script(&format!("handle_message({message})"))
                    .expect("valid script");
            }
            if let Some(StateChanged) = self.waveform_state_sub.try_recv() {
                let message = serde_json::to_string(&FrontendMessage::WaveformStateUpdated)
                    .expect("serializable");
                self.main_web_view
                    .evaluate_script(&format!("handle_message({message})"))
                    .expect("valid script");
            }

            match event {
                Event::LoopDestroyed => {
                    if let Some(player) = self.player.take() {
                        self.player_sub.broadcast(PlayerMessage::CommandQuit);
                        if let Err(err) = player.join() {
                            log::error!("{err}");
                        }
                    }
                    log::info!("bye!");
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => *control_flow = ControlFlow::Exit,

                _ => (),
            }

            if let Ok(event) = menu_event_receiver.try_recv() {
                if event.id == self.media_controls_menu.item_open.id() {
                    let picked = rfd::FileDialog::new()
                        .add_filter(
                            "Audio file or playlist",
                            &[
                                "m3u", "m3u8", "pls", "mp3", "flac", "ogg", "wav", "aac", "m4a",
                            ],
                        )
                        .set_title("Open audio file(s) or playlist")
                        .pick_files();
                    if let Some(picked) = picked {
                        self.frontend_sub.broadcast(FrontendMessage::LoadLocations {
                            locations: picked
                                .iter()
                                .map(|path| Utf8Path::from_path(path).unwrap().to_string())
                                .collect(),
                        });
                    }
                } else if event.id == self.media_controls_menu.item_show_hide_playlist.id() {
                    log::info!("TODO: show/hide playlist");
                }
            }

            if let Err(err) = self.healthcheck() {
                log::error!("{err}");
                rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Error)
                    .set_title("Fatal error")
                    .set_description(format!("{APP_TITLE} had a fatal error:\n{err}"))
                    .show();
                *control_flow = ControlFlow::ExitWithCode(1);
            }
        });
    }

    fn handle_player_messages(&self) {
        while let Some(message) = self.player_sub.try_recv() {
            if !message.frequent() {
                log::info!("ui-backend received broadcast message: {message:?}");
            }
            match message {
                PlayerMessage::UpdateWaveform(waveform) => {
                    let waveform_lock = waveform.lock().unwrap();
                    self.waveform_state.mutate(|state| {
                        state.waveform = Some(Waveform {
                            spectrum: waveform_lock.spectrum.into(),
                            amplitude: waveform_lock.amplitude.into(),
                        });
                    });
                }
                PlayerMessage::UpdatePlaybackStatus(status) => {
                    self.playback_state.mutate(|state| {
                        state.playback_status = status;
                    });
                }

                PlayerMessage::EventAudioDeviceCreationFailed(_err) => {
                    // TODO
                }
                PlayerMessage::EventAudioDeviceFailed(_err) => {
                    // TODO
                }
                PlayerMessage::EventFailedToDecodeAudio(_err) => {
                    // TODO
                }
                PlayerMessage::EventFailedToLoadLocation(_err) => {
                    // TODO
                }
                PlayerMessage::EventStartedTrack => {}
                PlayerMessage::EventFinishedTrack => {
                    self.waveform_state.mutate(|state| {
                        state.waveform = None;
                    });
                    self.playback_state.mutate(|state| {
                        state.playback_status = PlaybackStatus::default();
                        state.current_track = None;
                    });
                }
                PlayerMessage::EventMetadataLoaded(metadata) => {
                    self.playback_state.mutate(|state| {
                        state.current_track = Some(Track {
                            title: metadata.track_title,
                            artist: metadata.artist,
                            album: metadata.album,
                        });
                    });
                }

                _ => {}
            }
        }
    }

    fn handle_frontend_messages(&self) -> Option<ControlFlow> {
        while let Some(message) = self.frontend_sub.try_recv() {
            match message {
                FrontendMessage::Quit => return Some(ControlFlow::Exit),
                FrontendMessage::DragWindowStart => {
                    self.main_web_view.window().drag_window().unwrap();
                }
                FrontendMessage::MediaControlMenu => {
                    self.media_controls_menu.show(self.main_web_view.window());
                }
                FrontendMessage::ShowAlert { level, message } => {
                    let (level, title) = match level {
                        AlertLevel::Info => (rfd::MessageLevel::Info, ""),
                        AlertLevel::Warn => (rfd::MessageLevel::Warning, "Caution"),
                        AlertLevel::Error => (rfd::MessageLevel::Error, "Error"),
                    };
                    rfd::MessageDialog::new()
                        .set_level(level)
                        .set_title(title)
                        .set_description(&*message)
                        .show();
                }
                FrontendMessage::Log { level, message } => {
                    let level = match level {
                        LogLevel::Trace => log::Level::Trace,
                        LogLevel::Debug => log::Level::Debug,
                        LogLevel::Info => log::Level::Info,
                        LogLevel::Warn => log::Level::Warn,
                        LogLevel::Error => log::Level::Error,
                    };
                    log::log!(level, "[wasm] {message}");
                }
                _ => {}
            }
        }
        None
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
        use muda::{AboutMetadata, Submenu};

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
    ui_broadcaster: Broadcaster<FrontendMessage>,
    internal_protocol: Rc<InternalProtocol>,
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
                let internal_protocol = internal_protocol.clone();
                internal_protocol.handle_request(request)
            }
        })
        .with_ipc_handler({
            let broadcaster = ui_broadcaster.clone();
            move |_window, message| {
                match serde_json::from_str::<FrontendMessage>(&message) {
                    Ok(message) => {
                        broadcaster.broadcast(message);
                    }
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
                ui_broadcaster.broadcast(FrontendMessage::LoadLocations { locations });
            }
            true
        })
        .with_transparent(true)
        .build()
        .map_err(|err| FatalError::new("failed to create web view", err))?;
    Ok(webview)
}
