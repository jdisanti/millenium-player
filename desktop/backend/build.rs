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

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-changed=../frontend/static/app-icon/app-icon.ico");

        let mut resource = winres::WindowsResource::new();
        resource.set_icon("../frontend/static/app-icon/app-icon.ico");
        resource.compile().unwrap();
    }
}
