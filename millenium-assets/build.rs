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

use std::process::{Command, Stdio};

fn main() {
    // Ignore file changes in debug mode since debug loads the files from the file system.
    // https://doc.rust-lang.org/cargo/reference/build-scripts.html#rerun-if-changed
    #[cfg(debug_assertions)]
    {
        println!("cargo:rerun-if-changed=build.rs");
    }

    Command::new("npm")
        .arg("install")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .unwrap();

    for target in ["install", "clean", "build", "copy"] {
        Command::new("npm")
            .arg("run")
            .arg(target)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .unwrap();
    }
}
