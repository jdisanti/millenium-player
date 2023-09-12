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

use std::mem::size_of;

/// Copy the given slice of f32s into the given vec of bytes using native endianness.
pub fn copy_f32s_into_ne_bytes(into: &mut Vec<u8>, data: &[f32]) {
    for &value in data {
        into.extend_from_slice(&value.to_ne_bytes()[..]);
    }
}

/// Convert native endian bytes to f32s.
pub fn ne_bytes_to_f32s(bytes: &[u8]) -> Box<[f32]> {
    let mut f32s = Vec::with_capacity(bytes.len() / size_of::<f32>());
    for chunk in bytes.chunks_exact(size_of::<f32>()) {
        f32s.push(f32::from_ne_bytes(chunk.try_into().unwrap()));
    }
    f32s.into_boxed_slice()
}
