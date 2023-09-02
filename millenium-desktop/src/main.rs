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

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use crate::error::FatalError;
use std::env;

const APP_TITLE: &str = "Millenium Player";

/// Command-line argument parsing.
mod args;

/// Common error types.
mod error;

/// Inter-process communication with the UI's web view.
mod ipc;

/// Web view UI.
mod ui;

fn do_main() -> Result<(), FatalError> {
    let mode = args::parse(env::args_os())?;
    ui::Ui::new(mode)?.run();
}

fn main() {
    // TODO: Also log to file
    simplelog::CombinedLogger::init(vec![simplelog::TermLogger::new(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        simplelog::TerminalMode::Stderr,
        simplelog::ColorChoice::Auto,
    )])
    .expect("first and only logger init");

    let _ = do_main().map_err(|err| {
        log::error!("Fatal error: {err}");
        std::process::exit(1)
    });
}
