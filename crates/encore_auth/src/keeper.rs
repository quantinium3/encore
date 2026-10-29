use std::ffi::OsStr;
use std::io::{self, BufRead as _, BufReader, Write as _};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use encore_core::{AccountId, Session, SessionEvent};

use crate::process::{LoginProcess, ProcessError};
use crate::profiles::Profiles;
use crate::protocol::{COMMAND_KEEP_ALIVE, EXIT_NOT_SIGNED_IN, FLAG_DATA_DIR};

const RESTART_DELAY: Duration = Duration::from_secs(30);
const STOP_GRACE: Duration = Duration::from_secs(2);
const STOP_POLL: Duration = Duration::from_millis(50);

type OnEvent = dyn Fn(SessionEvent) + Send + Sync;

struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl Running {
    fn shut_down(mut self) {
        drop(self.stdin.take());
        let deadline = Instant::now() + STOP_GRACE;
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(STOP_POLL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Default)]
struct State {
    generation: u64,
    running: Option<Running>,
    session: Option<Session>,
}

enum Launch {
    Started(ChildStdout),
    Failed(ProcessError),
    MissingProfile(PathBuf),
    Stale,
}

enum Reaped {
    Stop,
    SignedOut,
    Restart,
}

struct Inner {
    process: LoginProcess,
    profiles: Profiles,
    on_event: Box<OnEvent>,
    restart_delay: Duration,
    state: Mutex<State>,
}

pub struct SessionKeeper {
    inner: Arc<Inner>,
}

impl SessionKeeper {
    pub fn new(
        binary: impl Into<PathBuf>,
        profiles: Profiles,
        on_event: impl Fn(SessionEvent) + Send + Sync + 'static,
    ) -> Self {
        Self::from_process(LoginProcess::new(binary), profiles, Box::new(on_event))
    }

    pub fn next_to_current_exe(
        profiles: Profiles,
        on_event: impl Fn(SessionEvent) + Send + Sync + 'static,
    ) -> io::Result<Self> {
        Ok(Self::from_process(
            LoginProcess::next_to_current_exe()?,
            profiles,
            Box::new(on_event),
        ))
    }

    fn from_process(process: LoginProcess, profiles: Profiles, on_event: Box<OnEvent>) -> Self {
        Self {
            inner: Arc::new(Inner {
                process,
                profiles,
                on_event,
                restart_delay: RESTART_DELAY,
                state: Mutex::new(State::default()),
            }),
        }
    }

    #[cfg(test)]
    fn with_restart_delay(mut self, delay: Duration) -> Self {
        Arc::get_mut(&mut self.inner).unwrap().restart_delay = delay;
        self
    }
}

impl encore_core::SessionKeeper for SessionKeeper {
    fn start(&self, account: &AccountId, session: &Session) {
        let (previous, generation) = {
            let mut state = self.inner.lock();
            state.generation += 1;
            state.session = Some(session.clone());
            (state.running.take(), state.generation)
        };
        if let Some(previous) = previous {
            previous.shut_down();
        }

        let inner = self.inner.clone();
        let account = account.clone();
        thread::spawn(move || inner.supervise(generation, &account));
    }

    fn stop(&self) {
        let running = {
            let mut state = self.inner.lock();
            state.generation += 1;
            state.running.take()
        };
        if let Some(running) = running {
            running.shut_down();
        }
    }
}

impl Drop for SessionKeeper {
    fn drop(&mut self) {
        encore_core::SessionKeeper::stop(self);
    }
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn is_stale(&self, generation: u64) -> bool {
        self.lock().generation != generation
    }

    fn supervise(&self, generation: u64, account: &AccountId) {
        loop {
            match self.launch(generation, account) {
                Launch::Stale => return,
                Launch::MissingProfile(path) => {
                    eprintln!(
                        "encore: session keeper: {}",
                        ProcessError::MissingProfile(path)
                    );
                    return;
                }
                Launch::Started(stdout) => {
                    self.forward(stdout, generation, account);
                    match self.reap(generation) {
                        Reaped::Stop => return,
                        Reaped::SignedOut => {
                            (self.on_event)(SessionEvent::SignedOut {
                                account: account.clone(),
                            });
                            return;
                        }
                        Reaped::Restart => {}
                    }
                }
                Launch::Failed(error) => eprintln!("encore: session keeper: {error}"),
            }

            thread::sleep(self.restart_delay);
            if self.is_stale(generation) {
                return;
            }
        }
    }

    fn launch(&self, generation: u64, account: &AccountId) -> Launch {
        let mut state = self.lock();
        if state.generation != generation {
            return Launch::Stale;
        }
        let Some(session) = state.session.clone() else {
            return Launch::Stale;
        };

        let profile = self.profiles.path(account);
        if !profile.is_dir() {
            return Launch::MissingProfile(profile);
        }

        let args: [&OsStr; 3] = [
            COMMAND_KEEP_ALIVE.as_ref(),
            FLAG_DATA_DIR.as_ref(),
            profile.as_os_str(),
        ];
        let mut child = match self.process.spawn(&args, Stdio::piped(), Stdio::inherit()) {
            Ok(child) => child,
            Err(error) => return Launch::Failed(error),
        };
        let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Launch::Stale;
        };

        let sent = writeln!(stdin, "{}", session.cookie()).and_then(|()| stdin.flush());
        if let Err(error) = sent {
            let _ = child.kill();
            let _ = child.wait();
            return Launch::Failed(ProcessError::Stdin(error));
        }

        state.running = Some(Running {
            child,
            stdin: Some(stdin),
        });
        Launch::Started(stdout)
    }

    fn forward(&self, stdout: ChildStdout, generation: u64, account: &AccountId) {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let cookie = line.trim();
            if cookie.is_empty() {
                continue;
            }

            let session = Session::new(cookie);
            {
                let mut state = self.lock();
                if state.generation != generation {
                    return;
                }
                state.session = Some(session.clone());
            }
            (self.on_event)(SessionEvent::Refreshed {
                account: account.clone(),
                session,
            });
        }
    }

    fn reap(&self, generation: u64) -> Reaped {
        let running = {
            let mut state = self.lock();
            if state.generation != generation {
                return Reaped::Stop;
            }
            state.running.take()
        };
        let Some(mut running) = running else {
            return Reaped::Stop;
        };

        drop(running.stdin.take());
        match running.child.wait().ok().and_then(|status| status.code()) {
            Some(0) => Reaped::Stop,
            Some(EXIT_NOT_SIGNED_IN) => Reaped::SignedOut,
            _ => Reaped::Restart,
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::sync::mpsc;

    use encore_core::SessionKeeper as _;

    use super::*;
    use crate::process::test_support::{fake_helper, serialize_fake_helpers};

    const WAIT: Duration = Duration::from_secs(5);

    struct Fixture {
        keeper: SessionKeeper,
        events: mpsc::Receiver<SessionEvent>,
        profiles: Profiles,
        _root: tempfile::TempDir,
    }

    fn alice() -> AccountId {
        AccountId::new("alice")
    }

    fn bob() -> AccountId {
        AccountId::new("bob")
    }

    fn fixture(helper: &Path, restart_delay: Duration, accounts: &[AccountId]) -> Fixture {
        let root = tempfile::tempdir().unwrap();
        let profiles = Profiles::new(root.path()).unwrap();
        for account in accounts {
            fs::create_dir_all(profiles.path(account)).unwrap();
        }
        let (sender, events) = mpsc::channel();
        let keeper = SessionKeeper::new(helper, profiles.clone(), move |event| {
            let _ = sender.send(event);
        })
        .with_restart_delay(restart_delay);
        Fixture {
            keeper,
            events,
            profiles,
            _root: root,
        }
    }

    fn cookie_of(event: SessionEvent) -> (AccountId, String) {
        match event {
            SessionEvent::Refreshed { account, session } => (account, session.into_cookie()),
            SessionEvent::SignedOut { account } => panic!("{account} signed out"),
        }
    }

    fn runs(counter: &Path) -> usize {
        fs::read_to_string(counter).map_or(0, |text| text.lines().count())
    }

    fn wait_for_runs(counter: &Path, wanted: usize) -> bool {
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            if runs(counter) >= wanted {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn sessions_the_helper_prints_reach_the_callback_tagged_with_the_account() {
        let _guard = serialize_fake_helpers();
        let dir = tempfile::tempdir().unwrap();
        let helper = fake_helper(
            dir.path(),
            r#"read line; echo "rotated-$line"; echo "again-$line"; cat >/dev/null"#,
        );
        let f = fixture(&helper, RESTART_DELAY, &[alice()]);

        f.keeper.start(&alice(), &Session::new("SAPISID=abc"));

        assert_eq!(
            cookie_of(f.events.recv_timeout(WAIT).unwrap()),
            (alice(), "rotated-SAPISID=abc".to_owned())
        );
        assert_eq!(
            cookie_of(f.events.recv_timeout(WAIT).unwrap()),
            (alice(), "again-SAPISID=abc".to_owned())
        );
        f.keeper.stop();
    }

    #[test]
    fn the_helper_gets_the_keep_alive_flag_and_the_profile_of_the_account() {
        let _guard = serialize_fake_helpers();
        let dir = tempfile::tempdir().unwrap();
        let helper = fake_helper(dir.path(), r#"echo "$1 $2 $3"; cat >/dev/null"#);
        let f = fixture(&helper, RESTART_DELAY, &[alice()]);

        f.keeper.start(&alice(), &Session::new("c"));

        assert_eq!(
            cookie_of(f.events.recv_timeout(WAIT).unwrap()).1,
            format!("keep-alive --data-dir {}", f.profiles.path(&alice()).display())
        );
        f.keeper.stop();
    }

    #[test]
    fn switching_account_moves_the_helper_to_that_accounts_profile() {
        let _guard = serialize_fake_helpers();
        let dir = tempfile::tempdir().unwrap();
        let helper = fake_helper(dir.path(), r#"read line; echo "$3"; cat >/dev/null"#);
        let f = fixture(&helper, RESTART_DELAY, &[alice(), bob()]);

        f.keeper.start(&alice(), &Session::new("c"));
        let first = cookie_of(f.events.recv_timeout(WAIT).unwrap());
        f.keeper.start(&bob(), &Session::new("c"));
        let second = cookie_of(f.events.recv_timeout(WAIT).unwrap());

        assert_eq!(
            first,
            (alice(), f.profiles.path(&alice()).display().to_string())
        );
        assert_eq!(
            second,
            (bob(), f.profiles.path(&bob()).display().to_string())
        );
        f.keeper.stop();
    }

    #[test]
    fn stopping_closes_stdin_so_the_helper_can_exit_on_its_own() {
        let _guard = serialize_fake_helpers();
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("exited-cleanly");
        let helper = fake_helper(
            dir.path(),
            &format!(
                r#"read line; echo up; cat >/dev/null; echo done > {}"#,
                marker.display()
            ),
        );
        let f = fixture(&helper, RESTART_DELAY, &[alice()]);
        f.keeper.start(&alice(), &Session::new("c"));
        f.events.recv_timeout(WAIT).unwrap();

        f.keeper.stop();

        let deadline = Instant::now() + WAIT;
        while !marker.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(marker.exists());
    }

    #[test]
    fn a_crashed_helper_is_started_again_with_the_latest_session() {
        let _guard = serialize_fake_helpers();
        let dir = tempfile::tempdir().unwrap();
        let counter = dir.path().join("runs");
        let helper = fake_helper(
            dir.path(),
            &format!(
                r#"read line; echo x >> {}; echo "seen-$line"; exit 1"#,
                counter.display()
            ),
        );
        let f = fixture(&helper, Duration::from_millis(20), &[alice()]);

        f.keeper.start(&alice(), &Session::new("first"));

        assert!(wait_for_runs(&counter, 2));
        let seen: Vec<_> = f
            .events
            .try_iter()
            .map(|event| cookie_of(event).1)
            .collect();
        assert_eq!(seen[0], "seen-first");
        assert_eq!(seen[1], "seen-seen-first");
        f.keeper.stop();
    }

    #[test]
    fn a_signed_out_helper_is_reported_and_not_started_again() {
        let _guard = serialize_fake_helpers();
        let dir = tempfile::tempdir().unwrap();
        let counter = dir.path().join("runs");
        let helper = fake_helper(
            dir.path(),
            &format!(
                "read line; echo x >> {}; exit {EXIT_NOT_SIGNED_IN}",
                counter.display()
            ),
        );
        let f = fixture(&helper, Duration::from_millis(20), &[alice()]);

        f.keeper.start(&alice(), &Session::new("c"));

        assert_eq!(
            f.events.recv_timeout(WAIT).unwrap(),
            SessionEvent::SignedOut { account: alice() }
        );
        thread::sleep(Duration::from_millis(300));
        assert_eq!(runs(&counter), 1);
        assert!(f.events.try_recv().is_err());
    }

    #[test]
    fn a_clean_exit_is_neither_reported_nor_started_again() {
        let _guard = serialize_fake_helpers();
        let dir = tempfile::tempdir().unwrap();
        let counter = dir.path().join("runs");
        let helper = fake_helper(
            dir.path(),
            &format!("read line; echo x >> {}; exit 0", counter.display()),
        );
        let f = fixture(&helper, Duration::from_millis(20), &[alice()]);

        f.keeper.start(&alice(), &Session::new("c"));

        assert!(wait_for_runs(&counter, 1));
        thread::sleep(Duration::from_millis(300));
        assert_eq!(runs(&counter), 1);
        assert!(f.events.try_recv().is_err());
    }

    #[test]
    fn a_stopped_keeper_does_not_start_the_helper_again_or_report_a_sign_out() {
        let _guard = serialize_fake_helpers();
        let dir = tempfile::tempdir().unwrap();
        let counter = dir.path().join("runs");
        let helper = fake_helper(
            dir.path(),
            &format!("read line; echo x >> {}; exit 1", counter.display()),
        );
        let f = fixture(&helper, Duration::from_millis(200), &[alice()]);
        f.keeper.start(&alice(), &Session::new("c"));
        assert!(wait_for_runs(&counter, 1));

        f.keeper.stop();

        thread::sleep(Duration::from_millis(500));
        assert_eq!(runs(&counter), 1);
        assert!(f.events.try_recv().is_err());
    }

    #[test]
    fn an_account_without_a_profile_never_starts_the_helper() {
        let _guard = serialize_fake_helpers();
        let dir = tempfile::tempdir().unwrap();
        let counter = dir.path().join("runs");
        let helper = fake_helper(dir.path(), &format!("echo x >> {}", counter.display()));
        let f = fixture(&helper, Duration::from_millis(20), &[]);

        f.keeper.start(&alice(), &Session::new("c"));

        thread::sleep(Duration::from_millis(300));
        assert_eq!(runs(&counter), 0);
        assert!(f.events.try_recv().is_err());
    }

    #[test]
    fn a_missing_helper_does_not_panic_and_stops_cleanly() {
        let f = fixture(
            Path::new("/nonexistent/encore-login"),
            Duration::from_millis(20),
            &[alice()],
        );

        f.keeper.start(&alice(), &Session::new("c"));
        thread::sleep(Duration::from_millis(100));
        f.keeper.stop();
    }
}
