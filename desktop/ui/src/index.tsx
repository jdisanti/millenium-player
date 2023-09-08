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

import { Component, render } from "preact";
import { IpcFetchInterval, Message, MessageContents } from "./ipc";
import { Waveform } from "./waveform";
import { Time } from "./component/time";
import {
    MediaControlButton,
    MediaControlButtonPausePlay,
} from "./component/media-control";

interface Playing {
    title?: string;
    artist?: string;
    album?: string;
    status: {
        playing: boolean;
        position_secs: number;
        duration_secs?: number;
    };
}

const SimplePlayer = (props: { playing: Playing }) => {
    return (
        <>
            <p>
                {props.playing.artist} - {props.playing.title}
            </p>
            <p>{props.playing.album}</p>
            <p>
                <Time time_secs={props.playing.status.position_secs} />
                /
                <Time time_secs={props.playing.status.duration_secs || 0} />
            </p>
            <MediaControlButton type="MediaControlSkipBack" disabled={false} />
            <MediaControlButton type="MediaControlBack" disabled={false} />
            <MediaControlButtonPausePlay
                playing={props.playing.status.playing}
            />
            <MediaControlButton type="MediaControlForward" disabled={false} />
            <MediaControlButton
                type="MediaControlSkipForward"
                disabled={false}
            />
            <MediaControlButton
                type="MediaControlPlaylistModeNormal"
                disabled={false}
            />
        </>
    );
};

interface AppState {
    playing: Playing;
}
class App extends Component<object, AppState> {
    private fetch_playing_data: IpcFetchInterval | null = null;
    private message_handler_id: number | null = null;

    constructor() {
        super();
        this.state = {
            playing: { status: { playing: false, position_secs: 0 } },
        };
    }

    override componentDidMount(): void {
        this.fetch_playing_data = new IpcFetchInterval(
            1000,
            "/ipc/playing-data",
        )
            .on_success(async (response) => {
                const playing = (await response.json()) as Playing;
                console.log(playing);
                this.setState({ ...this.state, playing });
            })
            .on_failure((err) => {
                console.warn(err);
            })
            .start();
        this.message_handler_id = Message.push_message_handler(
            (msg: MessageContents) => {
                if (msg.kind == "state_updated") {
                    this.fetch_playing_data?.fetch_now();
                }
            },
        );
    }

    override componentWillUnmount(): void {
        this.fetch_playing_data?.stop();
        if (this.message_handler_id != null) {
            Message.remove_message_handler(this.message_handler_id);
        }
    }

    render() {
        return <SimplePlayer playing={this.state.playing} />;
    }
}

(() => {
    const $ = document.querySelector.bind(document);
    (window as any)["millenium"] = {
        Message,
    };
    Message.push_message_handler((msg: Message) => {
        console.info("received message: ", msg);
    });

    $(".title-bar .close")!.addEventListener("click", () => {
        Message.send({ kind: "Quit" });
    });
    $(".title-bar")!.addEventListener("mousedown", (event) => {
        let target = event.target as HTMLElement | null;
        while (target) {
            if (target.classList.contains("button-bar")) {
                return;
            }
            target = target.parentElement;
        }
        Message.send({ kind: "DragWindowStart" });
    });

    new Waveform($(".waveform")! as HTMLCanvasElement);

    render(<App />, $("#preact-app")!);
})();
