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

/// Thread broadcast messaging and subscription.
#[cfg(feature = "broadcast")]
pub mod broadcast;

/// Utilities for converting to byte slices and back.
pub mod bytes;

/// Frontend message types.
pub mod frontend;

/// State types.
#[cfg(feature = "broadcast")]
pub mod state;

/// Common types used across multiple sets of messages.
pub mod types;
