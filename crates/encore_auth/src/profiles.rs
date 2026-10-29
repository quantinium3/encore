use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use encore_core::{AccountId, ProfileStore};

static STAGED: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct Profiles {
    accounts: PathBuf,
    pending: PathBuf,
}

impl Profiles {
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref();
        let profiles = Self {
            accounts: root.join("accounts"),
            pending: root.join("pending"),
        };
        create_private_dir(root)?;
        create_private_dir(&profiles.accounts)?;
        create_private_dir(&profiles.pending)?;
        profiles.sweep_pending()?;
        Ok(profiles)
    }

    pub fn path(&self, id: &AccountId) -> PathBuf {
        self.accounts.join(dir_name(id))
    }

    pub fn stage(&self) -> io::Result<PathBuf> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let count = STAGED.fetch_add(1, Ordering::Relaxed);
        let dir = self
            .pending
            .join(format!("p-{}-{nanos}-{count}", process::id()));
        fs::create_dir(&dir)?;
        restrict(&dir)?;
        Ok(dir)
    }

    fn sweep_pending(&self) -> io::Result<()> {
        for entry in fs::read_dir(&self.pending)? {
            remove_dir_if_present(&entry?.path())?;
        }
        Ok(())
    }

    fn check_staged(&self, staged: &Path) -> io::Result<()> {
        match staged.parent() == Some(self.pending.as_path()) {
            true => Ok(()),
            false => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a staged login profile", staged.display()),
            )),
        }
    }
}

impl ProfileStore for Profiles {
    fn adopt(&self, staged: &Path, id: &AccountId) -> io::Result<()> {
        self.check_staged(staged)?;
        let dest = self.path(id);
        remove_dir_if_present(&dest)?;
        fs::rename(staged, dest)
    }

    fn discard(&self, staged: &Path) {
        if self.check_staged(staged).is_ok() {
            let _ = remove_dir_if_present(staged);
        }
    }

    fn remove(&self, id: &AccountId) -> io::Result<()> {
        remove_dir_if_present(&self.path(id))
    }
}

fn dir_name(id: &AccountId) -> String {
    let hex: String = id
        .as_str()
        .bytes()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("a-{hex}")
}

fn remove_dir_if_present(dir: &Path) -> io::Result<()> {
    match fs::remove_dir_all(dir) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    restrict(dir)
}

#[cfg(unix)]
fn restrict(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn restrict(_: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profiles() -> (tempfile::TempDir, Profiles) {
        let root = tempfile::tempdir().unwrap();
        let profiles = Profiles::new(root.path()).unwrap();
        (root, profiles)
    }

    fn id(raw: &str) -> AccountId {
        AccountId::new(raw)
    }

    fn staged_with(profiles: &Profiles, file: &str, contents: &str) -> PathBuf {
        let staged = profiles.stage().unwrap();
        fs::write(staged.join(file), contents).unwrap();
        staged
    }

    #[test]
    fn staged_profiles_are_distinct_empty_directories() {
        let (_root, profiles) = profiles();

        let first = profiles.stage().unwrap();
        let second = profiles.stage().unwrap();

        assert_ne!(first, second);
        assert!(first.is_dir() && second.is_dir());
        assert_eq!(fs::read_dir(&first).unwrap().count(), 0);
    }

    #[test]
    fn adopting_moves_the_staged_profile_to_the_account() {
        let (_root, profiles) = profiles();
        let staged = staged_with(&profiles, "cookies", "alice-session");

        profiles.adopt(&staged, &id("alice")).unwrap();

        assert!(!staged.exists());
        let adopted = profiles.path(&id("alice")).join("cookies");
        assert_eq!(fs::read_to_string(adopted).unwrap(), "alice-session");
    }

    #[test]
    fn adopting_again_replaces_the_previous_profile_of_that_account() {
        let (_root, profiles) = profiles();
        let old = staged_with(&profiles, "stale", "old");
        profiles.adopt(&old, &id("alice")).unwrap();
        let fresh = staged_with(&profiles, "cookies", "new");

        profiles.adopt(&fresh, &id("alice")).unwrap();

        let dir = profiles.path(&id("alice"));
        assert!(!dir.join("stale").exists());
        assert_eq!(fs::read_to_string(dir.join("cookies")).unwrap(), "new");
    }

    #[test]
    fn accounts_do_not_share_a_profile() {
        let (_root, profiles) = profiles();
        let alice = staged_with(&profiles, "cookies", "alice");
        let bob = staged_with(&profiles, "cookies", "bob");

        profiles.adopt(&alice, &id("alice")).unwrap();
        profiles.adopt(&bob, &id("bob")).unwrap();

        let read = |who: &str| fs::read_to_string(profiles.path(&id(who)).join("cookies"));
        assert_eq!(read("alice").unwrap(), "alice");
        assert_eq!(read("bob").unwrap(), "bob");
    }

    #[test]
    fn discarding_removes_only_the_staged_profile() {
        let (_root, profiles) = profiles();
        let kept = staged_with(&profiles, "cookies", "alice");
        profiles.adopt(&kept, &id("alice")).unwrap();
        let staged = profiles.stage().unwrap();

        profiles.discard(&staged);
        profiles.discard(&staged);

        assert!(!staged.exists());
        assert!(profiles.path(&id("alice")).is_dir());
    }

    #[test]
    fn removing_an_account_profile_is_idempotent() {
        let (_root, profiles) = profiles();
        let staged = profiles.stage().unwrap();
        profiles.adopt(&staged, &id("alice")).unwrap();

        profiles.remove(&id("alice")).unwrap();
        profiles.remove(&id("alice")).unwrap();

        assert!(!profiles.path(&id("alice")).exists());
    }

    #[test]
    fn only_staged_directories_can_be_adopted_or_discarded() {
        let (root, profiles) = profiles();
        let outside = root.path().join("accounts");
        fs::write(outside.join("keep"), "x").unwrap();

        assert!(profiles.adopt(&outside, &id("alice")).is_err());
        profiles.discard(&outside);

        assert!(outside.join("keep").exists());
    }

    #[test]
    fn hostile_ids_stay_inside_the_accounts_directory() {
        let (root, profiles) = profiles();

        for raw in ["", ".", "..", "../outside", "a/b", "a\\b"] {
            let path = profiles.path(&id(raw));

            assert_eq!(path.parent(), Some(root.path().join("accounts").as_path()));
            assert_ne!(path, root.path().join("accounts"), "{raw:?}");
        }
    }

    #[test]
    fn ids_that_differ_only_in_case_get_different_directories() {
        let (_root, profiles) = profiles();

        assert_ne!(profiles.path(&id("Ab")), profiles.path(&id("aB")));
    }

    #[test]
    fn leftover_staged_profiles_are_swept_but_account_profiles_are_kept() {
        let root = tempfile::tempdir().unwrap();
        let first = Profiles::new(root.path()).unwrap();
        let leftover = first.stage().unwrap();
        let staged = first.stage().unwrap();
        first.adopt(&staged, &id("alice")).unwrap();

        let second = Profiles::new(root.path()).unwrap();

        assert!(!leftover.exists());
        assert!(second.path(&id("alice")).is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn profile_directories_are_private() {
        use std::os::unix::fs::PermissionsExt as _;

        let (root, profiles) = profiles();
        let staged = profiles.stage().unwrap();

        for dir in [
            root.path().to_owned(),
            root.path().join("accounts"),
            root.path().join("pending"),
            staged,
        ] {
            let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{}", dir.display());
        }
    }
}
