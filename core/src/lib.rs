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

/// Audio support logic.
pub mod audio;

/// Thread broadcast messaging and subscription.
pub mod broadcast;

/// Location struct that represents file system or network locations.
pub mod location;

/// Audio player thread.
pub mod player;

/// Playlist management.
pub mod playlist;

/// Message types.
pub mod message;

/// Audio metadata/tags.
pub mod metadata;

/// Application state.
pub mod state;
