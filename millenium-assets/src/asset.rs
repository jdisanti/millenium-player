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

use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt;
use std::path::PathBuf;

trait AssetContent: Send + Sync {
    fn contents(&self) -> Result<Cow<'static, [u8]>, AssetError>;
}

enum SwitchingAssetContent {
    FileSystem(FileSystemAsset),
    EmbeddedAsset(EmbeddedAsset),
}

impl SwitchingAssetContent {
    fn contents(&self) -> Result<Cow<'static, [u8]>, AssetError> {
        match self {
            Self::FileSystem(asset) => asset.contents(),
            Self::EmbeddedAsset(asset) => asset.contents(),
        }
    }
}

impl fmt::Debug for SwitchingAssetContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileSystem(fs) => write!(f, "file-system path \"{}\" (debug mode)", fs.path),
            Self::EmbeddedAsset(_) => write!(f, "embedded"),
        }
    }
}

struct FileSystemAsset {
    path: &'static str,
}

impl AssetContent for FileSystemAsset {
    fn contents(&self) -> Result<Cow<'static, [u8]>, AssetError> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("build")
            .join(self.path);
        let contents = std::fs::read(&path)
            .map_err(|err| AssetError::new(format!("failed to read asset {path:?}"), err))?;
        Ok(Cow::Owned(contents))
    }
}

struct EmbeddedAsset {
    contents: &'static [u8],
}

impl AssetContent for EmbeddedAsset {
    fn contents(&self) -> Result<Cow<'static, [u8]>, AssetError> {
        Ok(Cow::Borrowed(self.contents))
    }
}

#[derive(Debug)]
pub struct Asset {
    mime: &'static str,
    contents: SwitchingAssetContent,
}

#[allow(dead_code)]
impl Asset {
    pub fn mime(&self) -> &'static str {
        self.mime
    }

    pub fn contents(&self) -> Result<Cow<'static, [u8]>, AssetError> {
        self.contents.contents()
    }

    pub(crate) const fn from_path_debug(mime: &'static str, path: &'static str) -> Self {
        Self {
            mime,
            contents: SwitchingAssetContent::FileSystem(FileSystemAsset { path }),
        }
    }

    pub(crate) const fn from_path_release(mime: &'static str, embedded: &'static [u8]) -> Self {
        Self {
            mime,
            contents: SwitchingAssetContent::EmbeddedAsset(EmbeddedAsset { contents: embedded }),
        }
    }
}

#[derive(Debug)]
pub struct AssetError {
    message: Cow<'static, str>,
    source: Option<Box<dyn StdError + Send + Sync>>,
}

impl StdError for AssetError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as _)
    }
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(source) = &self.source {
            write!(f, ": {source}")?;
        }
        Ok(())
    }
}

impl AssetError {
    pub(crate) fn new(
        message: impl Into<Cow<'static, str>>,
        source: impl StdError + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    pub(crate) fn msg(message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_should_fail_later() {
        let asset = Asset::from_path_debug("", "does-not-exist");
        let err = asset
            .contents()
            .expect_err("it should error on content retrieval");
        let message = err.to_string();
        assert!(
            message.contains("failed to read asset"),
            "expected '{message}' to contain 'failed to read asset'"
        );
        #[cfg(not(target_os = "windows"))]
        {
            assert!(
                message.contains("build/does-not-exist"),
                "expected '{message}' to contain 'build/does-not-exist'"
            );
        }
        #[cfg(target_os = "windows")]
        {
            assert!(
                message.contains("build\\\\does-not-exist"),
                "expected '{message}' to contain 'build\\\\does-not-exist'"
            );
        }
    }

    #[test]
    fn embedded_assets() {
        let contents = Asset::from_path_release("", b"test").contents().unwrap();
        assert_eq!(&b"test"[..], &*contents);
    }

    #[test]
    fn test_asset() {
        assert_eq!(&b"test"[..], &*crate::test::TEST_ASSET.contents().unwrap());
    }
}
