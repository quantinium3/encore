use std::path::{Path, PathBuf};

use anyhow::Context as _;

pub fn data_dir() -> anyhow::Result<PathBuf> {
    let dir = dirs::data_dir()
        .context("could not find the user data directory")?
        .join("encore");
    create_private_dir(&dir)?;
    Ok(dir)
}

fn create_private_dir(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("could not restrict {}", dir.display()))?;
    }
    Ok(())
}
