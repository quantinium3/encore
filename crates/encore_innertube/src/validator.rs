use std::sync::Mutex;

use encore_core::{AccountId, Session, SessionValidator, ValidationError, VerifiedSession, sapisid};
use serde_json::json;

use crate::error::Error;
use crate::identity::{
    AccountMenu, parse_account_menu, parse_selected_identity, parse_visitor_data,
};
use crate::transport::Transport;

const ACCOUNT_MENU: &str = "account/account_menu";
const ACCOUNTS_LIST: &str = "account/accounts_list";

pub struct InnertubeValidator {
    transport: Transport,
    visitor: Mutex<Option<String>>,
}

impl InnertubeValidator {
    pub fn new() -> Self {
        Self::with_transport(Transport::new())
    }

    fn with_transport(transport: Transport) -> Self {
        Self {
            transport,
            visitor: Mutex::new(None),
        }
    }

    #[cfg(test)]
    fn pointing_at(origin: String) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .proxy(None)
            .build();
        Self::with_transport(Transport::with_agent(
            ureq::Agent::new_with_config(config),
            origin,
        ))
    }

    fn anonymous_visitor(&self) -> Option<String> {
        let mut cached = self.visitor.lock().unwrap_or_else(|e| e.into_inner());
        if cached.is_none() {
            *cached = self
                .transport
                .fetch_visitor_page()
                .ok()
                .filter(|reply| (200..300).contains(&reply.status))
                .and_then(|reply| parse_visitor_data(&reply.body));
        }
        cached.clone()
    }

    fn listed_identity(
        &self,
        session: &Session,
        sapisid: &str,
        visitor: Option<&str>,
    ) -> Result<String, String> {
        let reply = self
            .transport
            .post(ACCOUNTS_LIST, session.cookie(), sapisid, visitor, json!({}))
            .map_err(|_| "the request failed".to_owned())?;
        match reply.status {
            200..=299 => parse_selected_identity(&reply.body)
                .map_err(|_| "not valid JSON".to_owned())?
                .ok_or_else(|| "no selectable identity".to_owned()),
            status => Err(format!("HTTP {status}")),
        }
    }
}

impl Default for InnertubeValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionValidator for InnertubeValidator {
    fn validate(&self, session: &Session) -> Result<VerifiedSession, ValidationError> {
        let Some(sapisid) = sapisid(session.cookie()) else {
            return Err(ValidationError::Rejected);
        };
        let anonymous = self.anonymous_visitor();

        let reply = self
            .transport
            .post(
                ACCOUNT_MENU,
                session.cookie(),
                sapisid,
                anonymous.as_deref(),
                json!({}),
            )
            .map_err(backend)?;
        let menu = match reply.status {
            200..=299 => {
                parse_account_menu(&reply.body).map_err(|error| backend(Error::Response(error)))?
            }
            401 | 403 => return Err(ValidationError::Rejected),
            status => return Err(backend(Error::Status(status))),
        };
        if menu.logged_out {
            return Err(ValidationError::Rejected);
        }

        let id = match menu.id() {
            Some(id) => id.to_owned(),
            None if !menu.has_header => {
                eprintln!(
                    "encore: account_menu named no account and had no account header ({})",
                    menu.shape
                );
                return Err(ValidationError::Rejected);
            }
            None => {
                let login_bound = menu.visitor_data.as_deref().or(anonymous.as_deref());
                self.listed_identity(session, sapisid, login_bound)
                    .map_err(|outcome| no_identity(&menu, &outcome))?
            }
        };

        Ok(VerifiedSession {
            id: AccountId::new(id),
            profile: menu.profile,
        })
    }
}

fn no_identity(menu: &AccountMenu, accounts_list: &str) -> ValidationError {
    backend(Error::NoIdentity(format!(
        "{}; accounts_list: {accounts_list}",
        menu.shape
    )))
}

fn backend(error: Error) -> ValidationError {
    ValidationError::Backend(Box::new(error))
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::thread;

    use super::*;

    const SW_JS_DATA: &str = ")]}'\n[[1,2,[\"noise\",\"CgtANONYMOUS\"]]]";

    fn menu_body(context: &str) -> String {
        format!(
            r#"{{
                "responseContext": {context},
                "actions": [{{"openPopupAction": {{"popup": {{"multiPageMenuRenderer": {{"sections": [{{
                    "activeAccountHeaderRenderer": {{
                        "accountName": {{"runs": [{{"text": "Personal channel"}}]}},
                        "channelHandle": {{"runs": [{{"text": "@personal"}}]}},
                        "accountPhoto": {{"thumbnails": [{{"url": "https://img.example/a.jpg"}}]}},
                        "navigationEndpoint": {{"browseEndpoint": {{"browseId": "UCPERSONAL"}}}}
                    }}
                }}]}}}}}}}}]
            }}"#
        )
    }

    fn menu_without_channel(context: &str) -> String {
        menu_body(context).replace("UCPERSONAL", "no-channel")
    }

    const LIST_ONE: &str = r#"{"actions": [{"x": {"accountSectionListRenderer": {"contents": [
        {"accountItem": {
            "accountName": {"runs": [{"text": "Default"}]},
            "isSelected": true,
            "hasChannel": true,
            "serviceEndpoint": {"selectActiveIdentityEndpoint": {"supportedTokens": [
                {"datasyncIdToken": {"datasyncIdToken": "listed-sync||device"}}
            ]}}
        }}
    ]}}}]}"#;

    struct Server {
        origin: String,
        requests: Arc<Mutex<Vec<String>>>,
    }

    impl Server {
        fn requests_to(&self, path: &str) -> Vec<String> {
            self.requests
                .lock()
                .unwrap()
                .iter()
                .filter(|request| {
                    request
                        .lines()
                        .next()
                        .is_some_and(|line| line.contains(path))
                })
                .map(|request| request.to_ascii_lowercase())
                .collect()
        }

        fn request_count(&self) -> usize {
            self.requests.lock().unwrap().len()
        }
    }

    fn serve(routes: Vec<(&'static str, &'static str, String)>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut head = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                    let end = line == "\r\n";
                    head.push_str(&line);
                    if end {
                        break;
                    }
                }
                let mut payload = vec![0; length];
                let _ = reader.read_exact(&mut payload);
                head.push_str(&String::from_utf8_lossy(&payload));
                let request_line = head.lines().next().unwrap_or_default().to_owned();
                seen.lock().unwrap().push(head);

                let (status, body) = routes
                    .iter()
                    .find(|(path, _, _)| request_line.contains(path))
                    .map_or(("404 Not Found", String::new()), |(_, status, body)| {
                        (*status, body.clone())
                    });
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        Server { origin, requests }
    }

    fn routes(menu: String) -> Vec<(&'static str, &'static str, String)> {
        vec![
            ("sw.js_data", "200 OK", SW_JS_DATA.to_owned()),
            ("account_menu", "200 OK", menu),
        ]
    }

    fn validator(server: &Server) -> InnertubeValidator {
        InnertubeValidator::pointing_at(server.origin.clone())
    }

    fn validate(server: &Server, cookie: &str) -> Result<VerifiedSession, ValidationError> {
        validator(server).validate(&Session::new(cookie))
    }

    #[test]
    fn the_datasync_id_in_the_response_context_is_the_account() {
        let server = serve(routes(menu_body(
            r#"{"mainAppWebResponseContext": {"datasyncId": "108123||"}}"#,
        )));

        let verified = validate(&server, "SAPISID=abc; SID=1").unwrap();

        assert_eq!(verified.id, AccountId::new("108123"));
        assert_eq!(verified.profile.name, "Personal channel");
        assert_eq!(verified.profile.detail.as_deref(), Some("@personal"));
        assert_eq!(
            verified.profile.avatar_url.as_deref(),
            Some("https://img.example/a.jpg")
        );
        assert!(server.requests_to("accounts_list").is_empty());
    }

    #[test]
    fn the_channel_id_is_the_account_when_the_context_has_no_datasync_id() {
        let server = serve(routes(menu_body("{}")));

        let verified = validate(&server, "SAPISID=abc").unwrap();

        assert_eq!(verified.id, AccountId::new("UCPERSONAL"));
        assert!(server.requests_to("accounts_list").is_empty());
    }

    #[test]
    fn the_same_account_keeps_its_id_when_the_cookie_rotates() {
        let server = serve(routes(menu_body("{}")));

        let before = validate(&server, "SAPISID=abc; __Secure-3PSIDTS=old").unwrap();
        let after = validate(&server, "__Secure-3PSIDTS=new; SAPISID=abc").unwrap();

        assert_eq!(before.id, after.id);
    }

    #[test]
    fn the_listed_identity_is_used_when_neither_the_context_nor_a_channel_names_the_account() {
        let mut routes = routes(menu_without_channel(r#"{"visitorData": "CgtLOGINBOUND"}"#));
        routes.push(("accounts_list", "200 OK", LIST_ONE.to_owned()));
        let server = serve(routes);

        let verified = validate(&server, "SAPISID=abc").unwrap();

        assert_eq!(verified.id, AccountId::new("listed-sync"));
        assert_eq!(verified.profile.name, "Personal channel");
    }

    #[test]
    fn the_account_list_is_asked_with_the_login_bound_visitor_not_the_anonymous_one() {
        let mut routes = routes(menu_without_channel(r#"{"visitorData": "CgtLOGINBOUND"}"#));
        routes.push(("accounts_list", "200 OK", LIST_ONE.to_owned()));
        let server = serve(routes);

        validate(&server, "SAPISID=abc").unwrap();

        let list = &server.requests_to("accounts_list")[0];
        assert!(
            list.contains("\r\nx-goog-visitor-id: cgtloginbound\r\n"),
            "{list}"
        );
        assert!(list.contains(r#""visitordata":"cgtloginbound""#), "{list}");
        assert!(!list.contains("cgtanonymous"), "{list}");
        assert!(list.contains("\r\nauthorization: sapisidhash "), "{list}");
    }

    #[test]
    fn the_visitor_bootstrap_is_anonymous_and_its_id_goes_on_the_menu_request() {
        let server = serve(routes(menu_body("{}")));

        validate(&server, "SAPISID=supersecretvalue").unwrap();

        let bootstrap = &server.requests_to("sw.js_data")[0];
        assert!(!bootstrap.contains("cookie:"), "{bootstrap}");
        assert!(!bootstrap.contains("authorization:"), "{bootstrap}");
        assert!(!bootstrap.contains("supersecretvalue"), "{bootstrap}");
        let menu = &server.requests_to("account_menu")[0];
        assert!(
            menu.contains("\r\nx-goog-visitor-id: cgtanonymous\r\n"),
            "{menu}"
        );
        assert!(menu.contains(r#""visitordata":"cgtanonymous""#), "{menu}");
    }

    #[test]
    fn the_visitor_bootstrap_happens_once_per_validator() {
        let server = serve(routes(menu_body("{}")));
        let validator = validator(&server);

        validator.validate(&Session::new("SAPISID=abc")).unwrap();
        validator.validate(&Session::new("SAPISID=abc")).unwrap();

        assert_eq!(server.requests_to("sw.js_data").len(), 1);
        assert_eq!(server.requests_to("account_menu").len(), 2);
    }

    #[test]
    fn a_failed_visitor_bootstrap_does_not_stop_validation() {
        let server = serve(vec![
            ("sw.js_data", "500 Internal Server Error", String::new()),
            ("account_menu", "200 OK", menu_body("{}")),
        ]);

        let verified = validate(&server, "SAPISID=abc").unwrap();

        assert_eq!(verified.id, AccountId::new("UCPERSONAL"));
        let menu = &server.requests_to("account_menu")[0];
        assert!(!menu.contains("x-goog-visitor-id"), "{menu}");
        assert!(!menu.contains("visitordata"), "{menu}");
    }

    #[test]
    fn the_menu_request_is_the_authorized_web_remix_account_menu_call() {
        let server = serve(routes(menu_body("{}")));

        validate(&server, "SAPISID=abc; SID=1").unwrap();

        let request = &server.requests_to("account_menu")[0];
        assert!(
            request
                .starts_with("post /youtubei/v1/account/account_menu?prettyprint=false http/1.1"),
            "{request}"
        );
        assert!(request.contains("\r\ncookie: sapisid=abc; sid=1\r\n"));
        assert!(request.contains("\r\nauthorization: sapisidhash "));
        assert!(request.contains("\r\nx-youtube-client-name: 67\r\n"));
        assert!(request.contains("\r\nx-origin: https://music.youtube.com\r\n"));
        assert!(request.contains("\r\ncontent-type: application/json\r\n"));
        assert!(request.contains(r#""clientname":"web_remix""#));
        assert!(!request.contains("onbehalfofuser"));
    }

    #[test]
    fn the_hash_is_built_from_the_sapisid_and_never_sends_it_bare() {
        let server = serve(routes(menu_body("{}")));

        validate(&server, "SAPISID=supersecretvalue").unwrap();

        let request = &server.requests_to("account_menu")[0];
        let authorization = request
            .lines()
            .find(|line| line.starts_with("authorization:"))
            .unwrap();
        assert!(!authorization.contains("supersecretvalue"));
    }

    #[test]
    fn unauthorized_and_forbidden_mean_the_session_was_rejected() {
        for status in ["401 Unauthorized", "403 Forbidden"] {
            let server = serve(vec![
                ("sw.js_data", "200 OK", SW_JS_DATA.to_owned()),
                ("account_menu", status, String::new()),
            ]);

            let result = validate(&server, "SAPISID=abc");

            assert!(matches!(result, Err(ValidationError::Rejected)), "{status}");
        }
    }

    #[test]
    fn a_logged_out_answer_means_the_session_was_rejected() {
        let server = serve(routes(menu_body(
            r#"{"mainAppWebResponseContext": {"loggedOut": true, "datasyncId": "108123||"}}"#,
        )));

        let result = validate(&server, "SAPISID=abc");

        assert!(matches!(result, Err(ValidationError::Rejected)));
    }

    #[test]
    fn an_answer_with_no_account_header_and_no_id_is_a_dead_session_and_asks_nothing_more() {
        let server = serve(routes(
            r#"{"responseContext": {"serviceTrackingParams": []}}"#.to_owned(),
        ));

        let result = validate(&server, "SAPISID=abc");

        assert!(matches!(result, Err(ValidationError::Rejected)));
        assert!(server.requests_to("accounts_list").is_empty());
    }

    #[test]
    fn an_account_header_that_still_names_no_account_is_a_backend_failure_that_says_what_was_seen()
    {
        let mut routes = routes(menu_without_channel(r#"{"visitorData": "CgtLOGINBOUND"}"#));
        routes.push((
            "accounts_list",
            "200 OK",
            r#"{"responseContext": {}}"#.to_owned(),
        ));
        let server = serve(routes);

        let error = validate(&server, "SAPISID=supersecretvalue").unwrap_err();

        let text = format!("{error}");
        assert!(matches!(error, ValidationError::Backend(_)));
        assert!(text.contains("does not say which account"), "{text}");
        assert!(text.contains("account header: present"), "{text}");
        assert!(text.contains("no selectable identity"), "{text}");
        assert!(!format!("{error:?}").contains("supersecretvalue"));
    }

    #[test]
    fn a_failing_account_list_is_reported_not_hidden() {
        let mut routes = routes(menu_without_channel("{}"));
        routes.push(("accounts_list", "500 Internal Server Error", String::new()));
        let server = serve(routes);

        let error = validate(&server, "SAPISID=abc").unwrap_err();

        assert!(matches!(error, ValidationError::Backend(_)));
        assert!(format!("{error}").contains("HTTP 500"), "{error}");
    }

    #[test]
    fn a_server_error_is_a_backend_failure_not_a_rejection() {
        let server = serve(vec![
            ("sw.js_data", "200 OK", SW_JS_DATA.to_owned()),
            ("account_menu", "500 Internal Server Error", String::new()),
        ]);

        let error = validate(&server, "SAPISID=supersecretvalue").unwrap_err();

        assert!(matches!(error, ValidationError::Backend(_)));
        assert!(error.to_string().contains("HTTP 500"));
        assert!(!format!("{error:?}").contains("supersecretvalue"));
    }

    #[test]
    fn an_answer_that_is_not_json_is_a_backend_failure() {
        let server = serve(routes("<html>".to_owned()));

        let result = validate(&server, "SAPISID=abc");

        assert!(matches!(result, Err(ValidationError::Backend(_))));
    }

    #[test]
    fn an_unreachable_server_is_a_backend_failure() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);

        let result = InnertubeValidator::pointing_at(origin).validate(&Session::new("SAPISID=abc"));

        assert!(matches!(result, Err(ValidationError::Backend(_))));
    }

    #[test]
    fn a_session_without_sapisid_is_rejected_without_a_request() {
        let server = serve(routes(menu_body("{}")));

        for cookie in ["", "SID=1; PREF=x", "SAPISID="] {
            let result = validate(&server, cookie);

            assert!(
                matches!(result, Err(ValidationError::Rejected)),
                "{cookie:?}"
            );
        }
        assert_eq!(server.request_count(), 0);
    }
}
