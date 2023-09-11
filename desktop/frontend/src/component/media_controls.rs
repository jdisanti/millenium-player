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
use millenium_post_office::frontend::{message::FrontendMessage, state::PlaylistMode};
use yew::prelude::*;

#[derive(PartialEq)]
pub enum MediaControl {
    SkipBack,
    Back,
    Play,
    Pause,
    Forward,
    SkipForward,
    PlaylistMode(PlaylistMode),
}

impl MediaControl {
    fn aria_label(&self) -> String {
        fn playlist_mode(mode: &str) -> String {
            format!("Current playlist mode: {mode}. Click to change playlist mode.")
        }
        match self {
            Self::SkipBack => "Skip back".into(),
            Self::Back => "Back".into(),
            Self::Play => "Play".into(),
            Self::Pause => "Pause".into(),
            Self::Forward => "Forward".into(),
            Self::SkipForward => "Skip forward".into(),
            Self::PlaylistMode(mode) => match mode {
                PlaylistMode::Normal => playlist_mode("normal"),
                PlaylistMode::Shuffle => playlist_mode("shuffle"),
                PlaylistMode::RepeatOne => playlist_mode("repeat one"),
                PlaylistMode::RepeatAll => playlist_mode("repeat all"),
            },
        }
    }

    fn class_name(&self) -> &'static str {
        match self {
            Self::SkipBack => "media-control-skip-back",
            Self::Back => "media-control-back",
            Self::Play => "media-control-play",
            Self::Pause => "media-control-pause",
            Self::Forward => "media-control-forward",
            Self::SkipForward => "media-control-skip-forward",
            Self::PlaylistMode(mode) => match mode {
                PlaylistMode::Normal => "media-control-playlist-mode-normal",
                PlaylistMode::Shuffle => "media-control-playlist-mode-shuffle",
                PlaylistMode::RepeatOne => "media-control-playlist-mode-repeat-one",
                PlaylistMode::RepeatAll => "media-control-playlist-mode-repeat-all",
            },
        }
    }

    fn click_message(&self) -> FrontendMessage {
        match self {
            Self::SkipBack => FrontendMessage::MediaControlSkipBack,
            Self::Back => FrontendMessage::MediaControlBack,
            Self::Play => FrontendMessage::MediaControlPlay,
            Self::Pause => FrontendMessage::MediaControlPause,
            Self::Forward => FrontendMessage::MediaControlForward,
            Self::SkipForward => FrontendMessage::MediaControlSkipForward,
            Self::PlaylistMode(mode) => match mode {
                PlaylistMode::Normal => FrontendMessage::MediaControlPlaylistMode {
                    mode: PlaylistMode::Shuffle,
                },
                PlaylistMode::Shuffle => FrontendMessage::MediaControlPlaylistMode {
                    mode: PlaylistMode::RepeatOne,
                },
                PlaylistMode::RepeatOne => FrontendMessage::MediaControlPlaylistMode {
                    mode: PlaylistMode::RepeatAll,
                },
                PlaylistMode::RepeatAll => FrontendMessage::MediaControlPlaylistMode {
                    mode: PlaylistMode::Normal,
                },
            },
        }
    }
}

#[derive(Properties, PartialEq)]
pub struct MediaControlButtonProps {
    pub kind: MediaControl,
}

#[function_component(MediaControlButton)]
pub fn media_control_button(props: &MediaControlButtonProps) -> Html {
    let aria_label = props.kind.aria_label();
    let class = format!("media-control {}", props.kind.class_name());
    let click_message = props.kind.click_message();
    let onclick = move |_| post_message(&click_message);
    html! {
        <button aria-label={aria_label}
                class={class}
                onclick={onclick}>
            <i></i>
        </button>
    }
}

#[derive(Properties, PartialEq)]
pub struct MediaControlButtonPausePlayProps {
    pub playing: bool,
}

#[function_component(MediaControlButtonPausePlay)]
pub fn media_control_button_pause_play(props: &MediaControlButtonPausePlayProps) -> Html {
    let kind = if props.playing {
        MediaControl::Pause
    } else {
        MediaControl::Play
    };
    html! {
        <MediaControlButton kind={kind} />
    }
}

#[derive(Properties, PartialEq)]
pub struct MediaControlPlaylistModeProps {
    pub mode: PlaylistMode,
}

#[function_component(MediaControlPlaylistMode)]
pub fn media_control_playlist_mode(props: &MediaControlPlaylistModeProps) -> Html {
    let kind = MediaControl::PlaylistMode(props.mode);
    html! {
        <MediaControlButton kind={kind} />
    }
}
