use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub thumbnail_url: Option<String>,
    pub started_at: i64,
    pub ends_at: Option<i64>,
}

const DEFAULT_CLIENT_ID: &str = "1568735732196053012";

enum Command {
    Set(Activity),
    Clear,
}

#[derive(Clone)]
pub struct Presence {
    sender: Option<Sender<Command>>,
}

impl Presence {
    pub fn start() -> Self {
        let client_id = std::env::var("ENCORE_DISCORD_CLIENT_ID")
            .ok()
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_owned());
        let (sender, receiver) = channel();
        let spawned = std::thread::Builder::new()
            .name("encore-discord-presence".into())
            .spawn(move || run(client_id, receiver));
        match spawned {
            Ok(_) => Self {
                sender: Some(sender),
            },
            Err(error) => {
                eprintln!("encore: discord presence is disabled: {error}");
                Self { sender: None }
            }
        }
    }

    pub fn set(&self, activity: Activity) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(Command::Set(activity));
        }
    }

    pub fn clear(&self) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(Command::Clear);
        }
    }
}

const RECONNECT_DELAY: Duration = Duration::from_secs(15);

fn run(client_id: String, receiver: Receiver<Command>) {
    let mut client: Option<ipc::Client> = None;
    let mut pending: Option<Command> = None;
    loop {
        let command = match pending.take() {
            Some(command) => command,
            None => match receiver.recv() {
                Ok(command) => command,
                Err(_) => return,
            },
        };
        let command = drain_latest(command, &receiver);

        if client.is_none() {
            client = ipc::Client::connect(&client_id).ok();
        }
        let Some(connected) = client.as_mut() else {
            match receiver.recv_timeout(RECONNECT_DELAY) {
                Ok(next) => pending = Some(next),
                Err(RecvTimeoutError::Timeout) => pending = Some(command),
                Err(RecvTimeoutError::Disconnected) => return,
            }
            continue;
        };

        let result = match &command {
            Command::Set(activity) => connected.set_activity(Some(activity)),
            Command::Clear => connected.set_activity(None),
        };
        if result.is_err() {
            client = None;
            pending = Some(command);
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

fn drain_latest(mut command: Command, receiver: &Receiver<Command>) -> Command {
    while let Ok(next) = receiver.try_recv() {
        command = next;
    }
    command
}

#[cfg(unix)]
mod ipc {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;

    use anyhow::{Context as _, bail};
    use serde_json::{Value, json};

    use super::Activity;

    const OP_HANDSHAKE: u32 = 0;
    const OP_FRAME: u32 = 1;
    const OP_CLOSE: u32 = 2;
    const MAX_FRAME: usize = 1 << 20;

    pub struct Client {
        stream: UnixStream,
        nonce: u64,
    }

    fn socket_dirs() -> Vec<PathBuf> {
        let mut bases: Vec<PathBuf> = Vec::new();
        for key in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
            if let Some(value) = std::env::var_os(key) {
                bases.push(PathBuf::from(value));
            }
        }
        bases.push(PathBuf::from("/tmp"));
        let mut dirs = Vec::new();
        for base in bases {
            dirs.push(base.clone());
            dirs.push(base.join("app/com.discordapp.Discord"));
            dirs.push(base.join("app/com.discordapp.DiscordCanary"));
            dirs.push(base.join("snap.discord"));
            dirs.push(base.join("snap.discord-canary"));
        }
        dirs
    }

    impl Client {
        pub fn connect(client_id: &str) -> anyhow::Result<Self> {
            for dir in socket_dirs() {
                for index in 0..10 {
                    let path = dir.join(format!("discord-ipc-{index}"));
                    let Ok(stream) = UnixStream::connect(&path) else {
                        continue;
                    };
                    let mut client = Self { stream, nonce: 0 };
                    if client.handshake(client_id).is_ok() {
                        return Ok(client);
                    }
                }
            }
            bail!("discord is not running")
        }

        fn handshake(&mut self, client_id: &str) -> anyhow::Result<()> {
            self.stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
            self.stream
                .set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
            self.send(OP_HANDSHAKE, &json!({ "v": 1, "client_id": client_id }))?;
            let (opcode, reply) = self.receive()?;
            if opcode != OP_FRAME || reply.get("evt").and_then(Value::as_str) != Some("READY") {
                bail!("discord rejected the handshake");
            }
            Ok(())
        }

        pub fn set_activity(&mut self, activity: Option<&Activity>) -> anyhow::Result<()> {
            self.nonce += 1;
            let payload = json!({
                "cmd": "SET_ACTIVITY",
                "args": {
                    "pid": std::process::id(),
                    "activity": activity.map(activity_json),
                },
                "nonce": self.nonce.to_string(),
            });
            self.send(OP_FRAME, &payload)?;
            let (opcode, reply) = self.receive()?;
            if opcode == OP_CLOSE {
                bail!("discord closed the connection");
            }
            if reply.get("evt").and_then(Value::as_str) == Some("ERROR") {
                eprintln!("encore: discord refused the presence update: {reply}");
            }
            Ok(())
        }

        fn send(&mut self, opcode: u32, payload: &Value) -> anyhow::Result<()> {
            let body = serde_json::to_vec(payload)?;
            let mut frame = Vec::with_capacity(8 + body.len());
            frame.extend_from_slice(&opcode.to_le_bytes());
            frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
            frame.extend_from_slice(&body);
            self.stream
                .write_all(&frame)
                .context("could not write to discord")
        }

        fn receive(&mut self) -> anyhow::Result<(u32, Value)> {
            let mut header = [0u8; 8];
            self.stream
                .read_exact(&mut header)
                .context("could not read from discord")?;
            let opcode = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
            let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
            if length > MAX_FRAME {
                bail!("discord sent an oversized frame");
            }
            let mut body = vec![0u8; length];
            self.stream.read_exact(&mut body)?;
            let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            Ok((opcode, value))
        }
    }

    fn clamp(text: &str) -> String {
        let mut text: String = text.chars().take(128).collect();
        if text.chars().count() < 2 {
            text.push_str("  ");
        }
        text
    }

    fn activity_json(activity: &Activity) -> Value {
        let mut value = json!({
            "type": 2,
            "details": clamp(&activity.title),
            "timestamps": match activity.ends_at {
                Some(end) => json!({ "start": activity.started_at, "end": end }),
                None => json!({ "start": activity.started_at }),
            },
        });
        if let Some(artist) = &activity.artist {
            value["state"] = json!(clamp(artist));
        }
        if let Some(url) = &activity.thumbnail_url {
            let mut assets = json!({ "large_image": url });
            if let Some(album) = &activity.album {
                assets["large_text"] = json!(clamp(album));
            }
            value["assets"] = assets;
        }
        value
    }
}

#[cfg(not(unix))]
mod ipc {
    use super::Activity;

    pub struct Client;

    impl Client {
        pub fn connect(_client_id: &str) -> anyhow::Result<Self> {
            anyhow::bail!("discord presence is only supported on unix")
        }

        pub fn set_activity(&mut self, _activity: Option<&Activity>) -> anyhow::Result<()> {
            Ok(())
        }
    }
}
