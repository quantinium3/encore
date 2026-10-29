use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Output;

use encore_core::{AccountId, Login, LoginError, RefreshError, Session};

use crate::process::{LoginProcess, ProcessError};
use crate::profiles::Profiles;
use crate::protocol::{
    COMMAND_LOGIN, COMMAND_REFRESH, EXIT_CANCELLED, EXIT_NOT_SIGNED_IN, EXIT_TIMED_OUT,
    EXIT_UNCHANGED, FLAG_DATA_DIR,
};

pub struct LoginProvider {
    process: LoginProcess,
    profiles: Profiles,
}

impl LoginProvider {
    pub fn new(binary: impl Into<PathBuf>, profiles: Profiles) -> Self {
        Self {
            process: LoginProcess::new(binary),
            profiles,
        }
    }

    pub fn next_to_current_exe(profiles: Profiles) -> io::Result<Self> {
        Ok(Self {
            process: LoginProcess::next_to_current_exe()?,
            profiles,
        })
    }

    fn sign_in_at(&self, profile: &Path) -> Result<Session, LoginError> {
        let args: [&OsStr; 3] = [
            COMMAND_LOGIN.as_ref(),
            FLAG_DATA_DIR.as_ref(),
            profile.as_os_str(),
        ];

        let output = self.process.run(&args, None)?;
        match output.status.code() {
            Some(0) => Ok(Session::new(cookie_from(&output)?)),
            Some(EXIT_CANCELLED) => Err(LoginError::Cancelled),
            _ => Err(failed(&output).into()),
        }
    }
}

impl encore_core::LoginProvider for LoginProvider {
    fn login(&self) -> Result<Login, LoginError> {
        let profile = self.profiles.stage().map_err(ProcessError::Profile)?;
        match self.sign_in_at(&profile) {
            Ok(session) => Ok(Login { session, profile }),
            Err(error) => {
                encore_core::ProfileStore::discard(&self.profiles, &profile);
                Err(error)
            }
        }
    }
}

pub struct SessionRefresher {
    process: LoginProcess,
    profiles: Profiles,
}

impl SessionRefresher {
    pub fn new(binary: impl Into<PathBuf>, profiles: Profiles) -> Self {
        Self {
            process: LoginProcess::new(binary),
            profiles,
        }
    }

    pub fn next_to_current_exe(profiles: Profiles) -> io::Result<Self> {
        Ok(Self {
            process: LoginProcess::next_to_current_exe()?,
            profiles,
        })
    }
}

impl encore_core::SessionRefresher for SessionRefresher {
    fn refresh(&self, account: &AccountId, current: &Session) -> Result<Session, RefreshError> {
        let profile = self.profiles.path(account);
        if !profile.is_dir() {
            return Err(ProcessError::MissingProfile(profile).into());
        }

        let args: [&OsStr; 3] = [
            COMMAND_REFRESH.as_ref(),
            FLAG_DATA_DIR.as_ref(),
            profile.as_os_str(),
        ];

        let output = self.process.run(&args, Some(current.cookie()))?;
        match output.status.code() {
            Some(0) => Ok(Session::new(cookie_from(&output)?)),
            Some(EXIT_NOT_SIGNED_IN) => Err(RefreshError::NotSignedIn),
            Some(EXIT_UNCHANGED) => Err(RefreshError::Unchanged),
            Some(EXIT_TIMED_OUT) => Err(ProcessError::TimedOut.into()),
            _ => Err(failed(&output).into()),
        }
    }
}

fn cookie_from(output: &Output) -> Result<String, ProcessError> {
    let cookie = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if cookie.is_empty() {
        return Err(ProcessError::EmptyOutput);
    }
    Ok(cookie)
}

fn failed(output: &Output) -> ProcessError {
    ProcessError::Failed {
        status: output.status.to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;

    use encore_core::{LoginProvider as _, ProfileStore as _, SessionRefresher as _};

    use super::*;
    use crate::process::test_support::{fake_helper, serialize_fake_helpers};

    fn profiles() -> (tempfile::TempDir, Profiles) {
        let root = tempfile::tempdir().unwrap();
        let profiles = Profiles::new(root.path().join("profiles")).unwrap();
        (root, profiles)
    }

    fn alice() -> AccountId {
        AccountId::new("alice")
    }

    fn login_with(script: &str) -> Result<Login, LoginError> {
        login_in(script).2
    }

    fn login_in(script: &str) -> (tempfile::TempDir, Profiles, Result<Login, LoginError>) {
        let _guard = serialize_fake_helpers();
        let dir = tempfile::tempdir().unwrap();
        let helper = fake_helper(dir.path(), script);
        let (root, profiles) = profiles();
        let result = LoginProvider::new(helper, profiles.clone()).login();
        (root, profiles, result)
    }

    fn pending_profiles(profiles: &Profiles) -> usize {
        let staged = profiles.stage().unwrap();
        let pending = staged.parent().unwrap().to_owned();
        profiles.discard(&staged);
        fs::read_dir(pending).unwrap().count()
    }

    #[test]
    fn exit_zero_returns_the_trimmed_cookie() {
        let login = login_with("echo 'SAPISID=abc; SID=1'").unwrap();

        assert_eq!(login.session.cookie(), "SAPISID=abc; SID=1");
    }

    #[test]
    fn login_runs_in_a_fresh_staged_profile_that_it_hands_back() {
        let (_root, profiles, result) = login_in(r#"echo "$1 $2 $3""#);

        let login = result.unwrap();

        assert_eq!(
            login.session.cookie(),
            format!("login --data-dir {}", login.profile.display())
        );
        assert!(login.profile.is_dir());
        assert_eq!(
            login.profile.parent().unwrap().file_name().unwrap(),
            "pending"
        );
        assert_eq!(pending_profiles(&profiles), 1);
    }

    #[test]
    fn every_login_gets_its_own_profile() {
        let _guard = serialize_fake_helpers();
        let dir = tempfile::tempdir().unwrap();
        let helper = fake_helper(dir.path(), "echo cookie");
        let (_root, profiles) = profiles();
        let provider = LoginProvider::new(helper, profiles);

        let first = provider.login().unwrap();
        let second = provider.login().unwrap();

        assert_ne!(first.profile, second.profile);
    }

    #[test]
    fn the_cancelled_exit_code_maps_to_cancelled_and_leaves_no_profile() {
        let (_root, profiles, result) = login_in(&format!("exit {EXIT_CANCELLED}"));

        assert!(matches!(result, Err(LoginError::Cancelled)));
        assert_eq!(pending_profiles(&profiles), 0);
    }

    #[test]
    fn a_usage_error_exit_is_a_failure_not_a_cancel() {
        let result = login_with("echo 'unrecognized subcommand' >&2; exit 2");

        assert!(matches!(result, Err(LoginError::Backend(_))));
    }

    #[test]
    fn other_failures_carry_the_helper_stderr_and_leave_no_profile() {
        let (_root, profiles, result) = login_in("echo 'window broke' >&2; exit 1");

        let error = result.unwrap_err();

        assert!(matches!(error, LoginError::Backend(_)));
        assert!(error.to_string().contains("window broke"));
        assert_eq!(pending_profiles(&profiles), 0);
    }

    #[test]
    fn empty_output_on_success_is_an_error_and_leaves_no_profile() {
        let (_root, profiles, result) = login_in("exit 0");

        assert!(matches!(result, Err(LoginError::Backend(_))));
        assert_eq!(pending_profiles(&profiles), 0);
    }

    #[test]
    fn a_missing_helper_is_a_backend_error() {
        let (_root, profiles) = profiles();

        let result = LoginProvider::new("/nonexistent/encore-login", profiles).login();

        assert!(matches!(result, Err(LoginError::Backend(_))));
    }

    struct RefresherFixture {
        profiles: Profiles,
        refresher: SessionRefresher,
        _dirs: (tempfile::TempDir, tempfile::TempDir),
    }

    fn refresher_with(script: &str) -> RefresherFixture {
        let helper_dir = tempfile::tempdir().unwrap();
        let helper = fake_helper(helper_dir.path(), script);
        let (root, profiles) = profiles();
        fs::create_dir_all(profiles.path(&alice())).unwrap();
        RefresherFixture {
            refresher: SessionRefresher::new(helper, profiles.clone()),
            profiles,
            _dirs: (helper_dir, root),
        }
    }

    fn refresh_with(script: &str, current: &str) -> Result<Session, RefreshError> {
        let _guard = serialize_fake_helpers();
        refresher_with(script)
            .refresher
            .refresh(&alice(), &Session::new(current))
    }

    #[test]
    fn refresh_sends_the_current_cookie_on_stdin_and_returns_the_output() {
        let session = refresh_with(r#"read line; echo "fresh-$line""#, "SAPISID=abc").unwrap();

        assert_eq!(session.cookie(), "fresh-SAPISID=abc");
    }

    #[test]
    fn refresh_uses_the_profile_of_that_account() {
        let _guard = serialize_fake_helpers();
        let fixture = refresher_with(r#"echo "$1 $2 $3""#);

        let session = fixture
            .refresher
            .refresh(&alice(), &Session::new("SAPISID=abc"))
            .unwrap();

        assert_eq!(
            session.cookie(),
            format!(
                "refresh --data-dir {}",
                fixture.profiles.path(&alice()).display()
            )
        );
    }

    #[test]
    fn refresh_exit_codes_map_to_their_errors() {
        let unchanged = refresh_with(&format!("exit {EXIT_UNCHANGED}"), "c");
        let signed_out = refresh_with(&format!("exit {EXIT_NOT_SIGNED_IN}"), "c");

        assert!(matches!(unchanged, Err(RefreshError::Unchanged)));
        assert!(matches!(signed_out, Err(RefreshError::NotSignedIn)));
    }

    #[test]
    fn a_refresh_timeout_is_a_backend_error_not_a_sign_out() {
        let error = refresh_with(&format!("exit {EXIT_TIMED_OUT}"), "c").unwrap_err();

        assert!(matches!(error, RefreshError::Backend(_)));
        assert!(error.to_string().contains("timed out"));
    }

    #[test]
    fn refresh_failures_carry_the_helper_stderr() {
        let error = refresh_with("echo 'webview broke' >&2; exit 1", "c").unwrap_err();

        assert!(matches!(error, RefreshError::Backend(_)));
        assert!(error.to_string().contains("webview broke"));
    }

    #[test]
    fn refresh_with_a_missing_helper_is_a_backend_error() {
        let (_root, profiles) = profiles();
        fs::create_dir_all(profiles.path(&alice())).unwrap();
        let refresher = SessionRefresher::new("/nonexistent/encore-login", profiles);

        let result = refresher.refresh(&alice(), &Session::new("c"));

        assert!(matches!(result, Err(RefreshError::Backend(_))));
    }

    #[test]
    fn refreshing_an_account_without_a_profile_never_starts_the_helper() {
        let _guard = serialize_fake_helpers();
        let dir = tempfile::tempdir().unwrap();
        let ran = dir.path().join("ran");
        let helper = fake_helper(dir.path(), &format!("touch {}", ran.display()));
        let (_root, profiles) = profiles();
        let refresher = SessionRefresher::new(helper, profiles);

        let error = refresher
            .refresh(&alice(), &Session::new("c"))
            .unwrap_err();

        assert!(matches!(error, RefreshError::Backend(_)));
        assert!(error.to_string().contains("no saved login profile"));
        assert!(!ran.exists());
    }

    #[test]
    fn a_helper_that_exits_without_reading_stdin_still_reports_its_own_failure() {
        let oversized_cookie = "x".repeat(1_000_000);

        let error =
            refresh_with("echo 'bad arguments' >&2; exit 1", &oversized_cookie).unwrap_err();

        assert!(matches!(error, RefreshError::Backend(_)));
        assert!(error.to_string().contains("bad arguments"));
    }
}
