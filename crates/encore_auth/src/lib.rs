use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::thread;

use anyhow::{Context as _, bail};

pub const EXIT_CANCELLED: i32 = 10;
pub const EXIT_SIGNED_OUT: i32 = 11;

#[derive(Debug, PartialEq, Eq)]
pub enum Update {
    Cookie(String),
    SignedOut,
}

pub fn helper_path() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("could not find the encore executable")?;
    Ok(exe.with_file_name(format!("encore-login{}", std::env::consts::EXE_SUFFIX)))
}

pub fn login(helper: &Path, data_dir: &Path) -> anyhow::Result<Option<String>> {
    let output = helper_command(helper, data_dir, "login")
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("could not start {}", helper.display()))?;

    match output.status.code() {
        Some(0) => {
            let cookie = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if cookie.is_empty() {
                bail!("the login helper exited without a cookie");
            }
            Ok(Some(cookie))
        }
        Some(EXIT_CANCELLED) => Ok(None),
        _ => bail!(
            "the login helper failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    }
}

pub struct KeepAlive {
    _stdin: ChildStdin,
}

impl KeepAlive {
    pub fn spawn(
        helper: &Path,
        data_dir: &Path,
        mut on_update: impl FnMut(Update) + Send + 'static,
    ) -> anyhow::Result<Self> {
        let mut child = helper_command(helper, data_dir, "keep-alive")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .with_context(|| format!("could not start {}", helper.display()))?;
        let stdin = child.stdin.take().context("the helper has no stdin")?;
        let stdout = child.stdout.take().context("the helper has no stdout")?;

        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                on_update(Update::Cookie(line));
            }
            if child.wait().is_ok_and(|status| status.code() == Some(EXIT_SIGNED_OUT)) {
                on_update(Update::SignedOut);
            }
        });

        Ok(Self { _stdin: stdin })
    }
}

fn helper_command(helper: &Path, data_dir: &Path, command: &str) -> Command {
    let mut cmd = Command::new(helper);
    cmd.arg("--data-dir").arg(data_dir).arg(command);
    cmd
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;
    use std::sync::{Mutex, mpsc};
    use std::time::Duration;

    use super::*;

    static SPAWN_LOCK: Mutex<()> = Mutex::new(());

    fn with_helper<T>(script: &str, run: impl FnOnce(&Path) -> T) -> T {
        let _guard = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("encore-login");
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        run(&path)
    }

    fn login_with(script: &str) -> anyhow::Result<Option<String>> {
        with_helper(script, |helper| login(helper, Path::new("/data")))
    }

    fn keep_alive_with(script: &str, stop_after: usize) -> Vec<Update> {
        with_helper(script, |helper| {
            let (sender, receiver) = mpsc::channel();
            let keep_alive = KeepAlive::spawn(helper, Path::new("/data"), move |update| {
                let _ = sender.send(update);
            })
            .unwrap();
            let mut updates: Vec<_> = receiver.iter().take(stop_after).collect();
            drop(keep_alive);
            updates.extend(receiver.recv_timeout(Duration::from_secs(5)));
            updates
        })
    }

    #[test]
    fn login_passes_the_data_dir_and_returns_the_trimmed_cookie() {
        let cookie = login_with(r#"echo " $1 $2 $3 ""#).unwrap();

        assert_eq!(cookie.as_deref(), Some("--data-dir /data login"));
    }

    #[test]
    fn closing_the_login_window_is_not_an_error() {
        assert_eq!(login_with("exit 10").unwrap(), None);
    }

    #[test]
    fn login_failures_carry_the_helper_stderr() {
        let error = login_with("echo 'window broke' >&2; exit 1").unwrap_err();

        assert!(error.to_string().contains("window broke"));
    }

    #[test]
    fn login_without_a_cookie_is_an_error() {
        assert!(login_with("exit 0").is_err());
    }

    #[test]
    fn keep_alive_forwards_cookies_and_reports_sign_out() {
        let updates = keep_alive_with(r#"echo "$3"; echo b; exit 11"#, 2);

        assert_eq!(
            updates,
            [
                Update::Cookie("keep-alive".into()),
                Update::Cookie("b".into()),
                Update::SignedOut
            ]
        );
    }

    #[test]
    fn dropping_keep_alive_closes_the_helper_stdin() {
        let updates = keep_alive_with("echo up; cat >/dev/null; exit 11", 1);

        assert_eq!(updates, [Update::Cookie("up".into()), Update::SignedOut]);
    }
}
