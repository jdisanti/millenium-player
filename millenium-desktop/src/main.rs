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
use std::{env, path::PathBuf};

const APP_TITLE: &str = "Millenium Player";
const APP_NAME: &str = "millenium-player";

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
    initialize_logging();

    let _ = do_main().map_err(|err| {
        log::error!("Fatal error: {err}");
        std::process::exit(1)
    });
}

/// Creates a terminal logger and log file, and initializes the default logger.
fn initialize_logging() {
    use simplelog::{
        ColorChoice, CombinedLogger, LevelFilter, SharedLogger, TermLogger, TerminalMode,
    };

    let mut loggers: Vec<Box<dyn SharedLogger>> = Vec::new();

    // Set up terminal logging first
    loggers.push(TermLogger::new(
        LevelFilter::Info,
        logger_config(),
        TerminalMode::Stderr,
        ColorChoice::Auto,
    ));

    // While setting up a log file, we can encounter errors, so we need a way to output those.
    // Thus, make a macro that directly outputs to the already created terminal logger.
    macro_rules! log_term {
        ($level:ident, $fmt:expr $(, $arg:tt)*) => {
            loggers[0].log(
                &log::Record::builder()
                    .level(log::Level::$level)
                    .args(format_args!($fmt, $($arg)*))
                    .module_path_static(Some(module_path!()))
                    .file_static(Some(file!()))
                    .line(Some(line!()))
                    .build(),
            )
        };
    }

    // Set up a log file
    {
        match create_file_logger() {
            Ok((logger, log_file_path)) => {
                log_term!(Info, "log file created at {log_file_path:?}");
                loggers.push(logger);
            }
            Err((err, Some(log_file_path))) => log_term!(
                Error,
                "failed to create log file at {log_file_path:?}: {err}"
            ),
            Err((err, None)) => log_term!(Error, "failed to create log file: {err}"),
        }
    }

    // Finally, initialize the default logger
    CombinedLogger::init(loggers).expect("first and only logger init");
}

fn create_file_logger(
) -> Result<(Box<dyn simplelog::SharedLogger>, PathBuf), (String, Option<PathBuf>)> {
    use simplelog::{LevelFilter, WriteLogger};
    use std::fs::{create_dir_all, File};

    let parent_path = dirs::cache_dir()
        .ok_or_else(|| (format!("failed to locate cache dir"), None))?
        .join(APP_NAME);
    let path = parent_path.join(format!("{APP_NAME}.log"));
    create_dir_all(&parent_path).map_err(|err| {
        (
            format!("parent path creation failed: {err}"),
            Some(path.clone()),
        )
    })?;
    let file = File::create(&path).map_err(|err| (format!("{err}"), Some(path.clone())))?;
    Ok((
        WriteLogger::new(LevelFilter::Info, logger_config(), file) as _,
        path,
    ))
}

fn logger_config() -> simplelog::Config {
    use simplelog::{format_description, ConfigBuilder, LevelFilter, ThreadLogMode, ThreadPadding};

    let mut builder = ConfigBuilder::new();
    builder
        .set_time_format_custom(format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ))
        .set_thread_level(LevelFilter::Info)
        .set_thread_padding(ThreadPadding::Left(6))
        .set_thread_mode(ThreadLogMode::Names);
    // Don't care if setting local offset fails
    let _ = builder.set_time_offset_to_local();
    builder.build()
}
