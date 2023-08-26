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

#![warn(unreachable_pub)]

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
    (pub(crate) $name:ident => $path:literal / $doc:literal) => {
        #[doc = $doc]
        pub(crate) static $name: Lazy<Asset> = Lazy::new(|| asset!($path));
    };
    (pub $name:ident => $path:literal / $doc:literal) => {
        #[doc = $doc]
        pub static $name: Lazy<Asset> = Lazy::new(|| asset!($path));
    };
    ($($name:ident => $path:literal / $doc:literal,)+) => {
        $(asset!(pub $name => $path / $doc);)+
        static ASSETS: Lazy<HashMap<&'static str, &'static Lazy<Asset>>> =
            Lazy::<HashMap<&'static str, &'static Lazy<Asset>>>::new(|| {
                let mut assets = HashMap::new();
                $(assets.insert($path, &$name);)+
                assets
            });
    };
}

asset! {
    CSS_STYLE => "style.css" / "The CSS file for the UI.",
    FONT_CANTARELL => "cantarell/Cantarell-VF.otf" / "The main font for the UI.",
    HTML_SIMPLE_MODE => "simple_mode.html" / "The HTML file for simple mode.",
    JS_INDEX => "index.js" / "The JavaScript entry point.",
}

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
    use super::*;
    asset!(pub(crate) TEST_ASSET => "test_asset.txt" / "Asset for unit testing.");
}
