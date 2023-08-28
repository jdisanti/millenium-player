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

interface WaveformData {
    spectrum: number[];
    amplitude: number[];
}

export class Waveform {
    private ctx: CanvasRenderingContext2D | null = null;
    private width: number;
    private height: number;

    private waveforms: WaveformData[] = [
        { spectrum: [], amplitude: [] },
        { spectrum: [], amplitude: [] },
    ];
    private interpolation: number = 0;
    private interpolation_interval: any;

    constructor(private canvas: HTMLCanvasElement) {
        this.ctx = canvas.getContext("2d");
        this.width = canvas.width;
        this.height = canvas.height;

        this.interpolation_interval = setInterval(() => {
            this.interpolation = Math.min(1, this.interpolation + 1 / 13);
            this.draw();
        }, 13);

        Message.push_message_handler((msg: Message) => {
            if (msg.kind == "WaveformData") {
                this.interpolation = 0;
                this.waveforms[0] = this.waveforms[1];
                this.waveforms[1] = msg.data as WaveformData;
            }
        });
    }

    private draw() {
        const c = this.ctx;
        const waves = this.waveforms;
        const length = waves[0].spectrum.length;
        const interp = this.interpolation;
        if (!c || length != waves[1].spectrum.length) {
            return;
        }

        c.clearRect(0, 0, this.canvas.width, this.canvas.height);

        const step = this.width / length;
        const center_y = this.height * 0.66;
        for (let i = 0; i < length; i++) {
            let interp_spectrum = waves[0].spectrum[i] * (1 - interp) + waves[1].spectrum[i] * interp;
            let interp_amplitude = waves[0].amplitude[i] * (1 - interp) + waves[1].amplitude[i] * interp;

            const x = i * step;
            let y = center_y - interp_spectrum * (this.height * 0.6);
            let h = center_y - y;
            draw_choppy_gradient_up(c, x, y, step - 1, h);

            y = center_y;
            h = interp_amplitude * (this.height * 0.25);
            draw_choppy_gradient_down(c, x, y, step - 1, h);
        }
    }
}

function draw_choppy_gradient_up(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number) {
    const step = h / 4;
    for (let i = 0; i < 4; i++) {
        ctx.fillStyle = `rgba(${255 * ((4 - i) / 4)}, 0, 0)`;
        ctx.fillRect(x, y + step * i, w, step);
    }
}

function draw_choppy_gradient_down(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number) {
    const step = h / 3;
    for (let i = 0; i < 3; i++) {
        ctx.fillStyle = `rgba(${255 * ((i + 1) / 3)}, 0, 0)`;
        ctx.fillRect(x, y + step * i, w, step);
    }
}