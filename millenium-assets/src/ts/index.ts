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

import { Message } from "./ipc";
import { Waveform } from "./waveform";

function $(selector: string): HTMLElement | null {
    return document.querySelector(selector);
}

(function () {
    const w = window as any;
    w.millenium = {
        Message,
    };
    Message.push_message_handler((msg: Message) => {
        console.log("received message: ", msg);
    });

    $(".title-bar .close")!.addEventListener("click", () => {
        Message.send("Quit", null);
    });
    $(".title-bar")!.addEventListener("mousedown", (event) => {
        let target = event.target as HTMLElement | null;
        while (target) {
            if (target.classList.contains("button-bar")) {
                return;
            }
            target = target.parentElement;
        }
        Message.send("DragWindowStart", null);
    });

    const waveform_canvas = $(".waveform")! as HTMLCanvasElement;
    new Waveform(waveform_canvas);
})();
