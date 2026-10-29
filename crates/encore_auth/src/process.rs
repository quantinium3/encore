use std::env::consts::EXE_SUFFIX;
use std::ffi::OsStr;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

use encore_core::{LoginError, RefreshError};
use thiserror::Error;

use crate::protocol::LOGIN_BINARY;

#[derive(Debug, Error)]
pub(crate) enum ProcessError {
    #[error("could not start the login process at {path}: {source}")]
    Spawn {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not send the session to the login process: {0}")]
    Stdin(#[source] io::Error),
    #[error("could not wait for the login process: {0}")]
    Wait(#[source] io::Error),
    #[error("the login process exited with {status}: {stderr}")]
    Failed { status: String, stderr: String },
    #[error("the login process finished without returning a session")]
    EmptyOutput,
    #[error("the login process timed out waiting for the page to load")]
    TimedOut,
    #[error("could not prepare a login profile: {0}")]
    Profile(#[source] io::Error),
    #[error("there is no saved login profile at {}", .0.display())]
    MissingProfile(PathBuf),
}

impl From<ProcessError> for LoginError {
    fn from(error: ProcessError) -> Self {
        Self::Backend(Box::new(error))
    }
}

impl From<ProcessError> for RefreshError {
    fn from(error: ProcessError) -> Self {
        Self::Backend(Box::new(error))
    }
}

pub(crate) struct LoginProcess {
    path: PathBuf,
}

impl LoginProcess {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub(crate) fn next_to_current_exe() -> io::Result<Self> {
        let exe = std::env::current_exe()?;
        let dir = exe.parent().unwrap_or_else(|| Path::new("."));
        Ok(Self::new(dir.join(format!("{LOGIN_BINARY}{EXE_SUFFIX}"))))
    }

    pub(crate) fn spawn(
        &self,
        args: &[&OsStr],
        stdin: Stdio,
        stderr: Stdio,
    ) -> Result<Child, ProcessError> {
        let mut command = Command::new(&self.path);
        command
            .args(args)
            .stdin(stdin)
            .stdout(Stdio::piped())
            .stderr(stderr);
        hide_console_window(&mut command);
        command.spawn().map_err(|source| ProcessError::Spawn {
            path: self.path.clone(),
            source,
        })
    }

    pub(crate) fn run(&self, args: &[&OsStr], stdin: Option<&str>) -> Result<Output, ProcessError> {
        let mut child = self.spawn(
            args,
            match stdin {
                Some(_) => Stdio::piped(),
                None => Stdio::null(),
            },
            Stdio::piped(),
        )?;

        if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
            match writeln!(pipe, "{text}") {
                Err(error) if error.kind() != io::ErrorKind::BrokenPipe => {
                    return Err(ProcessError::Stdin(error));
                }
                _ => {}
            }
        }
        child.wait_with_output().map_err(ProcessError::Wait)
    }
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

#[cfg(all(test, unix))]
pub(crate) mod test_support {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Mutex, MutexGuard};

    use super::*;

    static FAKE_HELPER_LOCK: Mutex<()> = Mutex::new(());

    pub fn serialize_fake_helpers() -> MutexGuard<'static, ()> {
        FAKE_HELPER_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn fake_helper(dir: &Path, script: &str) -> PathBuf {
        let path = dir.join("fake-helper");
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}
