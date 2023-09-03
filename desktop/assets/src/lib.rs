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
use std::{borrow::Cow, collections::HashMap};

mod asset;
pub use asset::AssetError;

macro_rules! asset {
    ($mime:literal, $path:literal) => {
        {
            #[cfg(debug_assertions)]
            { crate::asset::Asset::from_path_debug($mime, $path) }
            #[cfg(not(debug_assertions))]
            { crate::asset::Asset::from_path_release($mime, include_bytes!(concat!("../../ui/build/", $path))) }
        }
    };
    (pub(crate) $name:ident => $path:literal / $mime:literal / $doc:literal) => {
        #[doc = $doc]
        pub(crate) static $name: Lazy<Asset> = Lazy::new(|| asset!($mime, $path));
    };
    (pub $name:ident => $path:literal / $mime:literal / $doc:literal) => {
        #[doc = $doc]
        pub static $name: Lazy<Asset> = Lazy::new(|| asset!($mime, $path));
    };
    ($($name:ident => $path:literal / $mime:literal / $doc:literal,)+) => {
        $(asset!(pub $name => $path / $mime / $doc);)+
        static ASSETS: Lazy<HashMap<&'static str, &'static Lazy<Asset>>> =
            Lazy::<HashMap<&'static str, &'static Lazy<Asset>>>::new(|| {
                let mut assets = HashMap::new();
                $(assets.insert($path, &$name);)+
                assets
            });
    };
}

asset! {
    CSS_STYLE => "style.css" / "text/css" / "The CSS file for the UI.",
    FONT_CANTARELL => "cantarell/Cantarell-VF.otf" / "font/otf" / "The main font for the UI.",
    HTML_INDEX => "index.html" / "text/html" / "The root HTML file for the UI.",
    ICON_CIRCLE => "material-symbols/circle.svg" / "image/svg+xml" / "Circle icon used for the traffic light in MacOS.",
    ICON_CLOSE => "material-symbols/close.svg" / "image/svg+xml" / "Close icon used for the close buttons on Windows and MacOS.",
    JS_INDEX => "index.js" / "text/javascript" / "The JavaScript entry point.",
}

pub struct LoadedAsset {
    pub mime: &'static str,
    pub contents: Cow<'static, [u8]>,
}

/// Returns the asset with the given name, or an error if it's not found.
pub fn asset(name: &str) -> Result<LoadedAsset, AssetError> {
    let asset = *ASSETS
        .get(name)
        .ok_or_else(|| AssetError::msg(format!("asset not found: {}", name)))?;
    let asset: &Asset = Lazy::force(asset);
    let contents = asset.contents()?;
    log::info!(
        "loaded asset \"{name}\" ({} bytes, {}): {asset:?}",
        contents.len(),
        asset.mime()
    );
    Ok(LoadedAsset {
        mime: asset.mime(),
        contents,
    })
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    asset!(pub(crate) TEST_ASSET => "test_asset.txt" / "text/plain" / "Asset for unit testing.");
}