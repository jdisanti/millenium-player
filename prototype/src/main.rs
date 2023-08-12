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

use std::{
    fs::File,
    io::{BufReader, Cursor, Read},
    time::Duration,
};

use rodio::Source;

fn main() {
    println!("loading hydrate.mp3 into memory");
    let mut file = BufReader::new(File::open("../test-data/hydrate/hydrate.mp3").unwrap());
    let mut contents = Vec::new();
    file.read_to_end(&mut contents).unwrap();

    println!("opening audio output device");
    let (_stream, stream_handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&stream_handle).unwrap();

    println!("playing first 5 seconds of hydrate.mp3");
    sink.append(
        rodio::Decoder::new(Cursor::new(contents.clone()))
            .unwrap()
            .take_duration(Duration::from_secs(5)),
    );
    sink.sleep_until_end();

    println!("skipping 60 seconds into hydrate.mp3");
    sink.append(
        rodio::Decoder::new(Cursor::new(contents.clone()))
            .unwrap()
            .skip_duration(Duration::from_secs(60)),
    );
    sink.sleep_until_end();
}
