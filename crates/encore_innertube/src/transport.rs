use std::time::Duration;

use serde_json::{Value, json};

use crate::auth::sapisid_hash;
use crate::error::Error;

pub(crate) const ORIGIN: &str = "https://music.youtube.com";

const REFERER: &str = "https://music.youtube.com/";
const CLIENT_NAME: &str = "WEB_REMIX";
const CLIENT_ID: &str = "67";
const CLIENT_VERSION: &str = "1.20260213.01.00";
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0";
const TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) struct Reply {
    pub status: u16,
    pub body: String,
}

pub(crate) struct Transport {
    agent: ureq::Agent,
    origin: String,
}

impl Transport {
    pub(crate) fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .build();
        Self::with_agent(ureq::Agent::new_with_config(config), ORIGIN.to_owned())
    }

    pub(crate) fn with_agent(agent: ureq::Agent, origin: String) -> Self {
        Self { agent, origin }
    }

    pub(crate) fn fetch_visitor_page(&self) -> Result<Reply, Error> {
        let mut response = self
            .agent
            .get(format!("{}/sw.js_data", self.origin))
            .header("user-agent", USER_AGENT)
            .header("referer", REFERER)
            .call()
            .map_err(Error::Http)?;
        read(&mut response)
    }

    pub(crate) fn post(
        &self,
        path: &str,
        cookie: &str,
        sapisid: &str,
        visitor: Option<&str>,
        extra: Value,
    ) -> Result<Reply, Error> {
        let url = format!("{}/youtubei/v1/{path}?prettyPrint=false", self.origin);
        let mut request = self
            .agent
            .post(&url)
            .header("content-type", "application/json")
            .header("accept", "application/json")
            .header("x-goog-api-format-version", "1")
            .header("x-youtube-client-name", CLIENT_ID)
            .header("x-youtube-client-version", CLIENT_VERSION)
            .header("x-origin", ORIGIN)
            .header("referer", REFERER)
            .header("user-agent", USER_AGENT)
            .header("cookie", cookie)
            .header("authorization", sapisid_hash(sapisid, ORIGIN));
        if let Some(visitor) = visitor {
            request = request.header("x-goog-visitor-id", visitor);
        }

        let mut body = context_body(visitor);
        match (&mut body, extra) {
            (Value::Object(base), Value::Object(fields)) => base.extend(fields),
            _ => {}
        }
        let mut response = request.send(body.to_string()).map_err(Error::Http)?;
        read(&mut response)
    }
}

fn read(response: &mut ureq::http::Response<ureq::Body>) -> Result<Reply, Error> {
    let status = response.status().as_u16();
    let body = match (200..300).contains(&status) {
        true => response.body_mut().read_to_string().map_err(Error::Http)?,
        false => String::new(),
    };
    Ok(Reply { status, body })
}

fn context_body(visitor: Option<&str>) -> Value {
    let mut client = json!({
        "clientName": CLIENT_NAME,
        "clientVersion": CLIENT_VERSION,
        "hl": "en",
        "gl": "US",
    });
    if let Some(visitor) = visitor {
        client["visitorData"] = Value::String(visitor.to_owned());
    }
    json!({
        "context": {
            "client": client,
            "request": {
                "internalExperimentFlags": [],
                "useSsl": true,
            },
            "user": {
                "lockedSafetyMode": false,
            },
        }
    })
}
