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

use std::time::Duration as StdDuration;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct DurationProps {
    pub duration: StdDuration,
}

#[function_component(Duration)]
pub fn duration(props: &DurationProps) -> Html {
    html! { <>{format(props.duration)}</> }
}

fn format(duration: StdDuration) -> String {
    let total_seconds = duration.as_secs();
    let hours = Some(total_seconds / 3600).filter(|&h| h > 0);
    let minutes = total_seconds % 3600 / 60;
    let seconds = total_seconds % 60;
    if let Some(hours) = hours {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {
        assert_eq!("00:01", format(StdDuration::from_secs(1)));
        assert_eq!("00:10", format(StdDuration::from_secs(10)));
        assert_eq!("01:01", format(StdDuration::from_secs(61)));
        assert_eq!("10:01", format(StdDuration::from_secs(601)));
        assert_eq!("59:59", format(StdDuration::from_secs(3599)));
        assert_eq!("1:00:00", format(StdDuration::from_secs(3600)));
        assert_eq!("1:01:01", format(StdDuration::from_secs(3661)));
    }
}
