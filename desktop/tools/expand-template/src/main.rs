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

use std::{collections::HashMap, ffi::OsString};

fn template_params(os: &'static str) -> HashMap<&'static str, &'static str> {
    let mut template_params = HashMap::new();
    template_params.insert("target_os", os);
    template_params.insert(
        "webview_background_color",
        if os == "windows" {
            // WebView2 seems to interpret the alpha backwards
            "rgba(0,0,0,1)"
        } else {
            "rgba(0,0,0,0)"
        },
    );
    template_params
}

fn do_main<Arg, Itr>(os: &'static str, args: Itr) -> Result<(), Box<dyn std::error::Error>>
where
    Arg: Into<OsString> + Clone,
    Itr: IntoIterator<Item = Arg>,
{
    let args = Args::parse(args)?;

    let input = std::fs::read_to_string(&args.input)
        .map_err(|err| format!("failed to read input: {err}"))?;

    let hb = handlebars::Handlebars::new();
    let output = hb.render_template(&input, &template_params(os))?;

    std::fs::write(&args.output, output).map_err(|err| format!("failed to write output: {err}"))?;
    Ok(())
}

fn main() {
    if let Err(err) = do_main(std::env::consts::OS, std::env::args_os()) {
        eprintln!("Fatal error: {err}");
        std::process::exit(1);
    }
}

#[derive(Debug)]
struct Args {
    input: String,
    output: String,
}

impl Args {
    #[allow(unused_mut)]
    fn parse<Arg, Itr>(args: Itr) -> Result<Self, clap::Error>
    where
        Arg: Into<OsString> + Clone,
        Itr: IntoIterator<Item = Arg>,
    {
        let matches = Self::cli_config().get_matches_from(args);
        let mut input = matches
            .get_one::<String>("INPUT")
            .expect("required")
            .as_str();
        let mut output = matches
            .get_one::<String>("OUTPUT")
            .expect("required")
            .as_str();

        // For some reason, on Windows when running under MinGW, this `\\?\` prefix is added to the file paths,
        // which Rust's standard library doesn't seem to have any knowledge about. Couldn't find any information
        // about this prefix since it's unsearchable, so just opting to remove it.
        #[cfg(target_os = "windows")]
        {
            if input.starts_with("\\\\?\\") {
                input = &input[4..];
            }
            if output.starts_with("\\\\?\\") {
                output = &output[4..];
            }
        }

        Ok(Args {
            input: input.into(),
            output: output.into(),
        })
    }

    fn cli_config() -> clap::Command {
        clap::Command::new("expand-template")
            .arg(clap::Arg::new("INPUT").required(true))
            .arg(clap::Arg::new("OUTPUT").required(true))
    }
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    #[test]
    fn test_cli_config() {
        let args = Args::parse(vec!["expand-template", "foo", "bar"]).unwrap();
        assert_eq!("foo", args.input);
        assert_eq!("bar", args.output);
    }

    fn test(expected: &str, os: &'static str) {
        let tmp_dir = tempfile::tempdir().unwrap();
        let tmp_input = tmp_dir.path().join("input");
        let tmp_output = tmp_dir.path().join("output");

        std::fs::write(
            &tmp_input,
            "some template with {{webview_background_color}} {{target_os}}",
        )
        .unwrap();

        let prog_name = OsString::from("dontcare");
        let args = [
            prog_name.as_os_str(),
            tmp_input.as_os_str(),
            tmp_output.as_os_str(),
        ];
        do_main(os, args).unwrap();

        let output = String::from_utf8(std::fs::read(tmp_output).unwrap()).unwrap();
        assert_eq!(expected, output);
    }

    #[test]
    fn test_full_expansion() {
        test("some template with rgba(0,0,0,0) linux", "linux");
        test("some template with rgba(0,0,0,0) macos", "macos");
        test("some template with rgba(0,0,0,1) windows", "windows");
    }
}
