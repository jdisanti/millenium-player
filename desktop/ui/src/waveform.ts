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

import { IpcFetchInterval } from "./ipc";

const DATA_REFRESHES_PER_SECOND = 30;
const DATA_REFRESH_INTERVAL = 1000 / DATA_REFRESHES_PER_SECOND;

interface WaveformData {
    spectrum: Float32Array;
    amplitude: Float32Array;
}

class WaveformRefresher {
    data: WaveformData = {
        spectrum: new Float32Array(0),
        amplitude: new Float32Array(0),
    };

    private fetcher: IpcFetchInterval;
    private on_refresh: (data: WaveformData) => void = () => {};

    constructor() {
        this.fetcher = new IpcFetchInterval(
            DATA_REFRESH_INTERVAL,
            "/ipc/waveform-data",
        )
            .on_success(async (response) => {
                const buf = await response.arrayBuffer();
                const vals = new Float32Array(buf, 0, buf.byteLength / 4);
                this.data = {
                    spectrum: vals.slice(0, vals.length / 2),
                    amplitude: vals.slice(vals.length / 2, vals.length),
                };
                this.on_refresh(this.data);
            })
            .on_failure((err) => {
                console.warn(err);
            })
            .start();
    }

    on_refresh_data(callback: (data: WaveformData) => void) {
        this.on_refresh = callback;
    }
}

class WaveformInterpolator {
    private first: WaveformData = {
        spectrum: new Float32Array(0),
        amplitude: new Float32Array(0),
    };
    private second: WaveformData = {
        spectrum: new Float32Array(0),
        amplitude: new Float32Array(0),
    };
    private interp: number = 0.0;
    private refresh_times: number[] = [];
    private average_time_between_refreshes: number = 0;
    private last_refresh: number = Date.now();

    feed(data: WaveformData) {
        this.refresh_times.push(Date.now() - this.last_refresh);
        this.last_refresh = Date.now();
        if (this.refresh_times.length > 4) {
            this.refresh_times.shift();
        }
        this.average_time_between_refreshes =
            this.refresh_times.reduce((a, b) => a + b) /
            this.refresh_times.length;

        this.first = this.second;
        this.second = data;
        this.interp = 0.0;
    }

    data(): WaveformData {
        if (this.interp >= 1.0) {
            return this.second;
        }
        const interp = this.interp;
        const first = this.first;
        const second = this.second;
        return {
            spectrum: first.spectrum.map(
                (v, i) => v * (1.0 - interp) + second.spectrum[i] * interp,
            ),
            amplitude: first.amplitude.map(
                (v, i) => v * (1.0 - interp) + second.amplitude[i] * interp,
            ),
        };
    }

    update(time_delta_millis: number) {
        const time_delta = time_delta_millis / 1000.0;
        this.interp +=
            time_delta * (this.average_time_between_refreshes / 30.0);
    }
}

export class Waveform {
    private ctx: CanvasRenderingContext2D | null = null;
    private width: number;
    private height: number;

    private refresher = new WaveformRefresher();
    private interpolator = new WaveformInterpolator();
    private last_draw: number = Date.now();

    constructor(private canvas: HTMLCanvasElement) {
        this.ctx = canvas.getContext("2d", { alpha: false });
        this.width = canvas.width;
        this.height = canvas.height;

        this.refresher.on_refresh_data((data) => {
            this.interpolator.feed(data);
        });
        window.requestAnimationFrame(this.draw.bind(this));
    }

    private draw(timestamp: DOMHighResTimeStamp) {
        const c = this.ctx;
        if (!c) {
            console.error("no graphics context for the waveform");
            return;
        }

        const time_delta_millis = timestamp - this.last_draw;
        this.last_draw = timestamp;
        this.interpolator.update(time_delta_millis);
        const waves = this.interpolator.data();
        const length = waves.spectrum.length;

        c.clearRect(0, 0, this.canvas.width, this.canvas.height);

        const step = Math.round(this.width / length);
        const center_y = Math.floor(this.height * 0.66);
        for (let i = 0; i < length; i++) {
            const spectrum = waves.spectrum[i];
            const amplitude = waves.amplitude[i];

            const x = i * step;
            let y = center_y - spectrum * (this.height * 0.6);
            let h = center_y - y;
            draw_choppy_gradient_up(c, x, y, step - 1, h);

            y = center_y;
            h = amplitude * (this.height * 0.25);
            draw_choppy_gradient_down(c, x, y, step - 1, h);
        }

        window.requestAnimationFrame(this.draw.bind(this));
    }
}

function draw_choppy_gradient_up(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    w: number,
    h: number,
) {
    const step = h / 4;
    for (let i = 0; i < 4; i++) {
        ctx.fillStyle = `rgba(${255 * ((4 - i) / 4)}, 0, 0)`;
        ctx.fillRect(x, y + step * i, w, step);
    }
}

function draw_choppy_gradient_down(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    w: number,
    h: number,
) {
    const step = h / 3;
    for (let i = 0; i < 3; i++) {
        ctx.fillStyle = `rgba(${255 * ((i + 1) / 3)}, 0, 0)`;
        ctx.fillRect(x, y + step * i, w, step);
    }
}