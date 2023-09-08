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

import { Message, MessageContents } from "../ipc";

export const MediaControlButton = (props: {
    type: string;
    disabled: boolean;
}) => {
    const varieties: {
        [key: string]: {
            ariaLabel: string;
            className: string;
            onClick?: MessageContents;
        };
    } = {
        MediaControlSkipBack: {
            ariaLabel: "Skip back",
            className: "media-control-skip-back",
        },
        MediaControlBack: {
            ariaLabel: "Back",
            className: "media-control-back",
        },
        MediaControlPlay: {
            ariaLabel: "Play",
            className: "media-control-play",
        },
        MediaControlPause: {
            ariaLabel: "Pause",
            className: "media-control-pause",
        },
        MediaControlStop: {
            ariaLabel: "Stop",
            className: "media-control-stop",
        },
        MediaControlForward: {
            ariaLabel: "Forward",
            className: "media-control-forward",
        },
        MediaControlSkipForward: {
            ariaLabel: "Skip forward",
            className: "media-control-skip-forward",
        },
        MediaControlPlaylistModeNormal: {
            ariaLabel:
                "Current playlist mode: normal. Click to change playlist mode",
            className: "media-control-playlist-mode-normal",
            onClick: { kind: "MediaControlPlaylistMode", mode: "Shuffle" },
        },
        MediaControlPlaylistModeShuffle: {
            ariaLabel:
                "Current playlist mode: shuffle. Click to change playlist mode",
            className: "media-control-playlist-mode-shuffle",
            onClick: { kind: "MediaControlPlaylistMode", mode: "RepeatOne" },
        },
        MediaControlPlaylistModeRepeatOne: {
            ariaLabel:
                "Current playlist mode: repeat one. Click to change playlist mode",
            className: "media-control-playlist-mode-repeat-one",
            onClick: { kind: "MediaControlPlaylistMode", mode: "RepeatAll" },
        },
        MediaControlPlaylistModeRepeatAll: {
            ariaLabel:
                "Current playlist mode: repeat all. Click to change playlist mode",
            className: "media-control-playlist-mode-repeat-all",
            onClick: { kind: "MediaControlPlaylistMode", mode: "RepeatNormal" },
        },
    };
    const button = varieties[props.type];
    if (!button) {
        throw new Error(`Unknown media control type: ${props.type}`);
    }
    const onClick = Message.send.bind(
        null,
        button.onClick || { kind: props.type },
    );
    return (
        <button
            aria-label={button.ariaLabel}
            onClick={onClick}
            class={"media-control " + button.className}
            disabled={props.disabled}
        >
            <i></i>
        </button>
    );
};

export const MediaControlButtonPausePlay = (props: { playing: boolean }) => {
    const type = props.playing ? "MediaControlPause" : "MediaControlPlay";
    return <MediaControlButton type={type} disabled={false} />;
};

export type PlaylistMode = "Normal" | "Shuffle" | "RepeatOne" | "RepeatAll";
export const MediaControlPlaylistMode = (props: { mode: PlaylistMode }) => {
    return (
        <MediaControlButton
            type={`MedialControlPlaylistMode${props.mode}`}
            disabled={false}
        />
    );
};
