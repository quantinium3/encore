use gpui_kit::assets::{Assets, icon_assets};
use gpui_kit::{AssetSource, Result, SharedString};
use std::borrow::Cow;

icon_assets!(
    EncoreIcons,
    [
        House,
        ListMusic,
        ListPlus,
        MicVocal,
        Music,
        Repeat,
        Shuffle,
        SkipBack,
        SkipForward,
        Volume1,
        Volume2,
        VolumeX,
    ]
);

pub struct EncoreAssets;

impl AssetSource for EncoreAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match EncoreIcons.load(path)? {
            Some(bytes) => Ok(Some(bytes)),
            None => Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = Assets.list(path)?;
        paths.extend(EncoreIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
