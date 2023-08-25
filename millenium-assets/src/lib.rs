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

use crate::asset::Asset;
use once_cell::sync::Lazy;
use std::collections::HashMap;

mod asset;
pub use asset::AssetError;

macro_rules! asset {
    ($path:literal) => {
        {
            #[cfg(debug_assertions)]
            { crate::asset::Asset::from_path_debug($path) }
            #[cfg(not(debug_assertions))]
            { crate::asset::Asset::from_path_release(include_bytes!(concat!("../build/", $path))) }
        }
    };
    ($name:ident => $path:literal / $doc:literal) => {
        #[doc = $doc]
        pub(crate) static $name: once_cell::sync::Lazy<crate::asset::Asset> =
            once_cell::sync::Lazy::new(|| asset!($path));
    };
    ($($path:literal / $doc:literal,)+) => {
        once_cell::sync::Lazy::<std::collections::HashMap<&'static str, Asset>>::new(|| {
            [
                $(($path, asset!($path)),)+
            ].into_iter().collect()
        })
    };
}

static ASSETS: Lazy<HashMap<&'static str, Asset>> = asset! {
    "simple_mode.html" / "The HTML file for simple mode.",
};

/// Returns the asset with the given name, or an error if it's not found.
pub fn asset(name: &str) -> Result<Vec<u8>, AssetError> {
    ASSETS
        .get(name)
        .ok_or_else(|| AssetError::msg(format!("asset not found: {}", name)))
        .and_then(|asset| asset.contents().map(|c| (asset, c)))
        .map(|(asset, contents)| {
            log::info!(
                "loaded asset \"{name}\" ({} bytes): {asset:?}",
                contents.len()
            );
            contents
        })
}

#[cfg(test)]
pub(crate) mod test {
    asset!(TEST_ASSET => "test_asset.txt" / "Asset for unit testing.");
}
