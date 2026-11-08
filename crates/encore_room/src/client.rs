use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, bail};
use rand::Rng as _;

use crate::code::normalize_code;
use crate::protocol::{Intent, Snapshot};

const POLL_TIMEOUT: Duration = Duration::from_secs(35);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_BACKOFF: Duration = Duration::from_secs(10);
const FAILURES_BEFORE_LOST: u32 = 4;

#[derive(Debug, Clone)]
pub enum Update {
    Snapshot {
        snapshot: Snapshot,
        clock_offset_ms: f64,
    },
    Lost(String),
}

pub struct Room {
    code: String,
    base: String,
    client_id: String,
    stop: Arc<AtomicBool>,
    outbox: Sender<Intent>,
}

impl Room {
    pub fn join(base_url: &str, code: &str) -> anyhow::Result<(Self, Receiver<Update>)> {
        let code = normalize_code(code).context("that is not a valid room code")?;
        let base = base_url.trim_end_matches('/').to_owned();
        let client_id = random_id();
        let request_agent = agent(REQUEST_TIMEOUT);

        let (first, offset) = request(
            &request_agent,
            "POST",
            &format!("{base}/room/{code}/join?client={client_id}"),
            None,
        )?;

        let (updates, receiver) = channel();
        let last_rev = Arc::new(AtomicU64::new(first.state.rev));
        let stop = Arc::new(AtomicBool::new(false));
        updates
            .send(Update::Snapshot {
                snapshot: first,
                clock_offset_ms: offset,
            })
            .ok();

        spawn_poller(Poller {
            base: base.clone(),
            code: code.clone(),
            client_id: client_id.clone(),
            last_rev: last_rev.clone(),
            stop: stop.clone(),
            updates: updates.clone(),
        })?;

        let (outbox, queued) = channel::<Intent>();
        let sender = IntentSender {
            base: base.clone(),
            code: code.clone(),
            client_id: client_id.clone(),
            last_rev,
            stop: stop.clone(),
            updates,
            agent: request_agent,
        };
        std::thread::Builder::new()
            .name("encore-room-send".into())
            .spawn(move || {
                while let Ok(intent) = queued.recv() {
                    sender.deliver(intent);
                }
            })
            .context("could not start the room sender")?;

        Ok((
            Self {
                code,
                base,
                client_id,
                stop,
                outbox,
            },
            receiver,
        ))
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn send(&self, intent: Intent) {
        let _ = self.outbox.send(intent);
    }

    pub fn leave(&self) {
        if self.stop.swap(true, Ordering::SeqCst) {
            return;
        }
        let url = format!("{}/room/{}/leave?client={}", self.base, self.code, self.client_id);
        std::thread::spawn(move || {
            let _ = agent(REQUEST_TIMEOUT).post(&url).send_empty();
        });
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        self.leave();
    }
}

struct Poller {
    base: String,
    code: String,
    client_id: String,
    last_rev: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    updates: Sender<Update>,
}

fn spawn_poller(poller: Poller) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("encore-room-poll".into())
        .spawn(move || poller.run())
        .map(|_| ())
        .context("could not start the room listener")
}

impl Poller {
    fn run(self) {
        let agent = agent(POLL_TIMEOUT);
        let mut failures = 0u32;
        while !self.stop.load(Ordering::SeqCst) {
            let since = self.last_rev.load(Ordering::SeqCst);
            let url = format!(
                "{}/room/{}/state?since={since}&client={}",
                self.base, self.code, self.client_id
            );
            match request(&agent, "GET", &url, None) {
                Ok((snapshot, offset)) => {
                    failures = 0;
                    if !publish(&self.last_rev, &self.updates, snapshot, offset) {
                        return;
                    }
                }
                Err(error) => {
                    failures += 1;
                    if failures == FAILURES_BEFORE_LOST
                        && self.updates.send(Update::Lost(format!("{error:#}"))).is_err()
                    {
                        return;
                    }
                    let backoff = Duration::from_secs(1 << failures.min(4)).min(MAX_BACKOFF);
                    std::thread::sleep(backoff);
                }
            }
        }
    }
}

struct IntentSender {
    base: String,
    code: String,
    client_id: String,
    last_rev: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    updates: std::sync::mpsc::Sender<Update>,
    agent: ureq::Agent,
}

impl IntentSender {
    fn deliver(&self, intent: Intent) {
        if self.stop.load(Ordering::SeqCst) {
            return;
        }
        let url = format!(
            "{}/room/{}/intent?client={}",
            self.base, self.code, self.client_id
        );
        let body = match serde_json::to_string(&intent) {
            Ok(body) => body,
            Err(_) => return,
        };
        match request(&self.agent, "POST", &url, Some(body)) {
            Ok((snapshot, offset)) => {
                publish(&self.last_rev, &self.updates, snapshot, offset);
            }
            Err(error) => eprintln!("encore: could not send a room action: {error:#}"),
        }
    }
}

fn publish(
    last_rev: &AtomicU64,
    updates: &std::sync::mpsc::Sender<Update>,
    snapshot: Snapshot,
    clock_offset_ms: f64,
) -> bool {
    last_rev.fetch_max(snapshot.state.rev, Ordering::SeqCst);
    updates
        .send(Update::Snapshot {
            snapshot,
            clock_offset_ms,
        })
        .is_ok()
}

fn agent(timeout: Duration) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .build();
    ureq::Agent::new_with_config(config)
}

fn request(
    agent: &ureq::Agent,
    method: &str,
    url: &str,
    body: Option<String>,
) -> anyhow::Result<(Snapshot, f64)> {
    let sent = now_ms();
    let response = match (method, body) {
        ("GET", _) => agent.get(url).call(),
        (_, Some(body)) => agent
            .post(url)
            .header("content-type", "application/json")
            .send(body),
        (_, None) => agent.post(url).send_empty(),
    };
    let mut response = response.context("could not reach the room server")?;
    let received = now_ms();
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .context("could not read the room server's reply")?;
    if !(200..300).contains(&status) {
        bail!("the room server answered {status}: {}", text.trim());
    }
    let snapshot: Snapshot =
        serde_json::from_str(&text).context("the room server sent something unexpected")?;
    let offset = match method {
        "GET" => snapshot.server_now_ms - received,
        _ => snapshot.server_now_ms - (sent + received) / 2.,
    };
    Ok((snapshot, offset))
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0., |elapsed| elapsed.as_secs_f64() * 1000.)
}

fn random_id() -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut rng = rand::rng();
    (0..24)
        .map(|_| HEX[rng.random_range(0..HEX.len())] as char)
        .collect()
}
