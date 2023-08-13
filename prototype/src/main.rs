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

use clap::Parser;

mod play_audio;
use play_audio::prototype_play_audio;

mod drag_and_drop;
use drag_and_drop::prototype_eframe_drag_and_drop;

mod waveform;
use waveform::prototype_waveform;

#[derive(Debug, clap::Subcommand)]
enum Prototype {
    /// Runs the prototype for playing audio
    PlayAudio,
    /// Eframe drag and drop
    EframeDragAndDrop,
    /// Waveform display
    Waveform,
}

#[derive(Debug, Parser)]
#[command()]
struct Args {
    #[command(subcommand)]
    prototype: Prototype,
}

fn main() {
    let args = Args::parse();

    match args.prototype {
        Prototype::PlayAudio => prototype_play_audio(),
        Prototype::EframeDragAndDrop => prototype_eframe_drag_and_drop(),
        Prototype::Waveform => prototype_waveform(),
    }
}
