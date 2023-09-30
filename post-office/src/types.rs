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

const DEFAULT_VOLUME: f32 = 1.0;

/// New-type for playback volume.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize))]
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
pub struct Volume(u8);

impl Volume {
    /// Create a new `Volume`.
    pub fn new(value: u8) -> Self {
        Self(value)
    }

    /// Convert the volume to a floating point in range [0.0, 1.0].
    pub fn as_percentage(&self) -> f32 {
        self.0 as f32 / u8::MAX as f32
    }

    /// Convert a percentage in range [0.0, 1.0] to a `Volume`.
    ///
    /// If the percentage is outside the range, it will be clamped to the range.
    pub fn from_percentage(percentage: f32) -> Self {
        Self((f32::max(0.0, f32::min(1.0, percentage)) * u8::MAX as f32) as u8)
    }

    /// Minimum volume value.
    pub const fn min() -> Volume {
        Volume(0)
    }

    /// Maximum volume value.
    pub const fn max() -> Volume {
        Volume(u8::MAX)
    }
}

impl Default for Volume {
    fn default() -> Self {
        Self((DEFAULT_VOLUME * u8::MAX as f32) as u8)
    }
}

impl From<u8> for Volume {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<Volume> for u8 {
    fn from(value: Volume) -> Self {
        value.0
    }
}

impl From<&Volume> for u8 {
    fn from(value: &Volume) -> Self {
        value.0
    }
}
