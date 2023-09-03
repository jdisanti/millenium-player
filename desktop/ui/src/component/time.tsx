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

export const Time = (props: { time_secs: number }) => {
    let seconds = props.time_secs;

    const hours = Math.floor(seconds / 3600);
    seconds -= hours * 3600;

    const minutes = Math.floor(seconds / 60);
    seconds = Math.floor(seconds - minutes * 60);

    const fmt = (n: number) => n.toString().padStart(2, "0");
    if (hours > 0) {
        return (
            <>
                {fmt(hours)}:{fmt(minutes)}:{fmt(seconds)}
            </>
        );
    } else {
        return (
            <>
                {fmt(minutes)}:{fmt(seconds)}
            </>
        );
    }
};
