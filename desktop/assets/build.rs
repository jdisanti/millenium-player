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
    io,
    process::{self, Command, Stdio},
};

const fn ui_path() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../ui")
}

#[cfg(target_os = "windows")]
fn npm(args: &[&str]) -> Command {
    let mut command = Command::new("cmd");
    command.current_dir(ui_path());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());
    command
        .arg("/C")
        .arg(format!("npm {}", args.to_vec().join(" ")));
    eprintln!("running: {command:?}");
    command
}

#[cfg(not(target_os = "windows"))]
fn npm(args: &[&str]) -> Command {
    let mut command = Command::new("npm");
    command.current_dir(ui_path());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());
    command.args(args);
    eprintln!("running: {command:?}");
    command
}

fn main() {
    // Ignore file changes in debug mode since debug loads the files from the file system.
    // https://doc.rust-lang.org/cargo/reference/build-scripts.html#rerun-if-changed
    #[cfg(debug_assertions)]
    {
        println!("cargo:rerun-if-changed=build.rs");
    }

    npm(&["install"]).output().map_err(handle_error).unwrap();

    for target in ["install", "clean", "build", "copy"] {
        npm(&["run", target])
            .output()
            .map_err(handle_error)
            .unwrap();
    }
}

fn handle_error(error: io::Error) -> ! {
    if let io::ErrorKind::NotFound = error.kind() {
        eprintln!(
            "npm not found on PATH. Please install npm and try again.\n\nPATH: {}",
            std::env::var("PATH").unwrap_or_else(|_| String::from("<not set>"))
        );
    } else {
        eprintln!("unexpected error running npm: {}", error);
    }
    process::exit(1);
}
