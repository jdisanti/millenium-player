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

use clap::ArgAction;
use clap::{error::ErrorKind, ArgMatches};
use millenium_core::location::{Location, ParseLocationError};
use std::{ffi, str::FromStr};

#[derive(Debug)]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub enum Mode {
    Simple {
        locations: Vec<Location>,
    },
    Library {
        storage_path: Option<Location>,
        audio_path: Option<Location>,
    },
}

fn invalid_location(err: ParseLocationError) -> clap::Error {
    cli_config().error(ErrorKind::InvalidValue, err.to_string())
}

pub fn parse<Arg, Itr>(args: Itr) -> Result<Mode, clap::Error>
where
    Arg: Into<ffi::OsString> + Clone,
    Itr: IntoIterator<Item = Arg>,
{
    let matches = cli_config().try_get_matches_from(args)?;
    match matches.subcommand() {
        Some(("library", sub)) => {
            let storage_path = sub
                .get_one::<String>("storage-path")
                .map(|s| Location::from_str(s).map_err(invalid_location))
                .transpose()?;
            let audio_path = sub
                .get_one::<String>("audio-path")
                .map(|s| Location::from_str(s).map_err(invalid_location))
                .transpose()?;
            Ok(Mode::Library {
                storage_path,
                audio_path,
            })
        }
        Some(("simple", sub)) => parse_simple(sub),
        _ => parse_simple(&matches),
    }
}

fn parse_simple(matches: &ArgMatches) -> Result<Mode, clap::Error> {
    let locations: Result<Vec<Location>, ParseLocationError> = matches
        .get_many::<String>("LOCATIONS")
        .unwrap_or_default()
        .map(|s| Location::from_str(s))
        .collect();
    match locations {
        Ok(locations) => Ok(Mode::Simple { locations }),
        Err(err) => Err(invalid_location(err)),
    }
}

fn cli_config() -> clap::Command {
    clap::Command::new("Millenium Player")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Portable audio player and library manager")
        .args_conflicts_with_subcommands(true)
        .arg(
            clap::Arg::new("LOCATIONS")
                .help("List of files or URLs to play (audio files, playlist files, or both)")
                .action(clap::ArgAction::Append)
                .required(false),
        )
        .subcommand(
            clap::Command::new("simple")
                .about("Run in a simple audio player mode with no library management features")
                .arg(
                    clap::Arg::new("LOCATIONS")
                        .help(
                            "List of files or URLs to play (audio files, playlist files, or both)",
                        )
                        .action(clap::ArgAction::Append)
                        .required(false)
                        .index(1),
                ),
        )
        .subcommand(
            clap::Command::new("library")
                .about("Run in a full-featured library management mode")
                .arg(
                    clap::Arg::new("storage-path")
                        .help("Path to the directory where the library database will be stored")
                        .long("storage-path")
                        .action(ArgAction::Set)
                        .required(false),
                )
                .arg(
                    clap::Arg::new("audio-path")
                        .help("Path to the directory where the audio files are stored")
                        .long("audio-path")
                        .action(ArgAction::Set)
                        .required(false),
                ),
        )
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    #[test]
    fn no_args_runs_simple_mode() {
        pretty_assertions::assert_eq!(
            Mode::Simple {
                locations: Vec::new()
            },
            parse(&["millenium-player"]).expect("success"),
        );
        pretty_assertions::assert_eq!(
            Mode::Simple {
                locations: Vec::new()
            },
            parse(&["ungabunga"]).expect("success"),
        );
    }

    #[test]
    fn file_paths_run_simple_mode() {
        pretty_assertions::assert_eq!(
            Mode::Simple {
                locations: vec![Location::path("foo.mp3")],
            },
            parse(&["millenium-player", "foo.mp3"]).expect("success"),
        );
        pretty_assertions::assert_eq!(
            Mode::Simple {
                locations: vec![Location::from_str("https://example.com/test.mp3").unwrap()],
            },
            parse(&["millenium-player", "https://example.com/test.mp3"]).expect("success"),
        );
        pretty_assertions::assert_eq!(
            Mode::Simple {
                locations: vec![Location::path("foo.mp3")],
            },
            parse(&["millenium-player", "--", "foo.mp3"]).expect("success"),
        );
        pretty_assertions::assert_eq!(
            Mode::Simple {
                locations: vec![Location::path("simple")],
            },
            parse(&["millenium-player", "--", "simple"]).expect("success"),
        );
    }

    #[test]
    fn simple_mode() {
        pretty_assertions::assert_eq!(
            Mode::Simple {
                locations: Vec::new()
            },
            parse(&["millenium-player", "simple"]).expect("success"),
        );
        pretty_assertions::assert_eq!(
            Mode::Simple {
                locations: Vec::new()
            },
            parse(&["ungabunga", "simple"]).expect("success"),
        );

        let args = parse(&[
            "millenium-player",
            "simple",
            "path/to/foo.ogg",
            "https://example.com/bar.mp3",
            "path/to/playlist.m3u8",
        ])
        .expect("success");
        pretty_assertions::assert_eq!(
            Mode::Simple {
                locations: vec![
                    Location::from_str("path/to/foo.ogg").unwrap(),
                    Location::from_str("https://example.com/bar.mp3").unwrap(),
                    Location::from_str("path/to/playlist.m3u8").unwrap()
                ]
            },
            args
        );
    }

    #[test]
    fn library_mode() {
        pretty_assertions::assert_eq!(
            Mode::Library {
                storage_path: None,
                audio_path: None,
            },
            parse(&["millenium-player", "library"]).expect("success"),
        );

        pretty_assertions::assert_eq!(
            Mode::Library {
                storage_path: Some(Location::from_str("some/path").unwrap()),
                audio_path: None,
            },
            parse(&["millenium-player", "library", "--storage-path", "some/path"])
                .expect("success"),
        );

        pretty_assertions::assert_eq!(
            Mode::Library {
                storage_path: Some(Location::from_str("some/path").unwrap()),
                audio_path: Some(Location::from_str("some/audio/path").unwrap()),
            },
            parse(&[
                "millenium-player",
                "library",
                "--storage-path",
                "some/path",
                "--audio-path",
                "some/audio/path"
            ])
            .expect("success"),
        );

        pretty_assertions::assert_eq!(
            Mode::Library {
                storage_path: None,
                audio_path: Some(Location::from_str("some/audio/path").unwrap()),
            },
            parse(&[
                "millenium-player",
                "library",
                "--audio-path",
                "some/audio/path"
            ])
            .expect("success"),
        );
    }
}
