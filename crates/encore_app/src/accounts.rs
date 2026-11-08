use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use encore_auth::{LoginProvider, Profiles, SessionKeeper, SessionRefresher};
use encore_core::{
    Account, AccountId, AccountService, Accounts as _, EventOutcome, LoginError, SessionEvent,
    SessionStatus, SignInError,
};
use encore_innertube::InnertubeValidator;
use encore_storage::SqliteStore;
use encore_ui::{AccountStatus, AccountSummary, AccountsEvent, AccountsModel};
use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use futures::channel::oneshot;
use gpui_kit::{
    App, Entity, SharedString, Window,
    component::{WindowExt as _, notification::Notification},
};

type Service = AccountService<
    LoginProvider,
    InnertubeValidator,
    SessionRefresher,
    Profiles,
    Arc<SqliteStore>,
    SessionKeeper,
>;

pub(crate) const DATABASE_FILE: &str = "encore-app.sqlite";
const UNNAMED_ACCOUNT: &str = "YouTube Music account";
const MAX_MESSAGE_CHARS: usize = 200;
const WORKER_STOPPED: &str = "The background task stopped unexpectedly.";

pub struct Wiring {
    service: Arc<Service>,
    events: UnboundedReceiver<SessionEvent>,
}

pub fn build(data_dir: &Path) -> anyhow::Result<Wiring> {
    let profiles = Profiles::new(data_dir.join("login-profiles"))
        .context("could not prepare login profiles")?;
    let store = Arc::new(
        SqliteStore::new(data_dir.join(DATABASE_FILE)).context("could not open the database")?,
    );
    let (sender, events) = unbounded();
    let keeper = SessionKeeper::next_to_current_exe(profiles.clone(), move |event| {
        let _ = sender.unbounded_send(event);
    })
    .context("could not locate the login helper")?;

    let service = AccountService::new(
        LoginProvider::next_to_current_exe(profiles.clone())
            .context("could not locate the login helper")?,
        InnertubeValidator::new(),
        SessionRefresher::next_to_current_exe(profiles.clone())
            .context("could not locate the login helper")?,
        profiles,
        store,
        keeper,
    );
    Ok(Wiring {
        service: Arc::new(service),
        events,
    })
}

pub fn attach(wiring: Wiring, model: &Entity<AccountsModel>, window: &mut Window, cx: &mut App) {
    let Wiring { service, events } = wiring;

    refresh_model(&service, model, cx);

    window
        .subscribe(model, cx, {
            let service = service.clone();
            move |model, event: &AccountsEvent, window, cx| match event {
                AccountsEvent::SignInRequested => start_sign_in(&service, model, window, cx),
                AccountsEvent::SwitchRequested(id) => {
                    start_switch(&service, model, id.clone(), window, cx)
                }
                AccountsEvent::SignOutRequested => start_sign_out(&service, model, window, cx),
            }
        })
        .detach();

    start_restore(&service, model.clone(), window, cx);
    listen_for_session_events(service, model.clone(), events, window, cx);
}

fn start_sign_in(
    service: &Arc<Service>,
    model: Entity<AccountsModel>,
    window: &mut Window,
    cx: &mut App,
) {
    run_exclusive(
        service,
        model,
        window,
        cx,
        |service| service.sign_in(),
        |result, window, cx| match result {
            Some(Err(error)) => {
                if let Some(message) = sign_in_failure(&error) {
                    eprintln!("encore: sign-in failed: {}", chain(&error));
                    window.push_notification(Notification::error(message), cx);
                }
            }
            Some(Ok(_)) => {}
            None => window.push_notification(Notification::error(WORKER_STOPPED), cx),
        },
    );
}

fn start_switch(
    service: &Arc<Service>,
    model: Entity<AccountsModel>,
    id: SharedString,
    window: &mut Window,
    cx: &mut App,
) {
    let name = display_name(model.read(cx), &id);
    run_exclusive(
        service,
        model,
        window,
        cx,
        move |service| service.switch(&AccountId::new(id.as_str())),
        move |result, window, cx| match result {
            Some(Ok(SessionStatus::Expired(_))) => {
                window.push_notification(Notification::warning(expired_message(&name)), cx)
            }
            Some(Ok(_)) => {}
            Some(Err(error)) => window.push_notification(
                Notification::error(format!("Could not switch account: {}", chain(&error))),
                cx,
            ),
            None => window.push_notification(Notification::error(WORKER_STOPPED), cx),
        },
    );
}

fn start_sign_out(
    service: &Arc<Service>,
    model: Entity<AccountsModel>,
    window: &mut Window,
    cx: &mut App,
) {
    run_exclusive(
        service,
        model,
        window,
        cx,
        |service| service.sign_out(),
        |result, window, cx| match result {
            Some(Ok(())) => {}
            Some(Err(error)) => window.push_notification(
                Notification::error(format!("Could not sign out cleanly: {}", chain(&error))),
                cx,
            ),
            None => window.push_notification(Notification::error(WORKER_STOPPED), cx),
        },
    );
}

fn start_restore(
    service: &Arc<Service>,
    model: Entity<AccountsModel>,
    window: &mut Window,
    cx: &mut App,
) {
    let name = model
        .read(cx)
        .active()
        .map(|account| account.name.clone())
        .unwrap_or_default();
    run_exclusive(
        service,
        model,
        window,
        cx,
        |service| service.restore(),
        move |result, window, cx| match result {
            Some(Ok(SessionStatus::Expired(_))) => {
                window.push_notification(Notification::warning(expired_message(&name)), cx)
            }
            Some(Ok(_)) => {}
            Some(Err(error)) => {
                eprintln!("encore: could not restore the session: {}", chain(&error))
            }
            None => eprintln!("encore: {WORKER_STOPPED}"),
        },
    );
}

fn listen_for_session_events(
    service: Arc<Service>,
    model: Entity<AccountsModel>,
    mut events: UnboundedReceiver<SessionEvent>,
    window: &mut Window,
    cx: &mut App,
) {
    window
        .spawn(cx, async move |cx| {
            while let Some(event) = events.next().await {
                let worker = service.clone();
                let outcome = run_blocking(move || worker.apply(event)).await;
                let service = service.clone();
                let model = model.clone();
                cx.update(|window, cx| match outcome {
                    Some(Ok(EventOutcome::SignedOut(id))) => {
                        let name = display_name(model.read(cx), id.as_str());
                        window.push_notification(
                            Notification::warning(signed_out_message(&name)),
                            cx,
                        );
                        refresh_model(&service, &model, cx);
                    }
                    Some(Ok(EventOutcome::Saved | EventOutcome::Ignored)) => {}
                    Some(Err(error)) => {
                        eprintln!(
                            "encore: could not store a session update: {}",
                            chain(&error)
                        )
                    }
                    None => eprintln!("encore: {WORKER_STOPPED}"),
                })
                .ok();
            }
        })
        .detach();
}

fn run_exclusive<T: Send + 'static>(
    service: &Arc<Service>,
    model: Entity<AccountsModel>,
    window: &mut Window,
    cx: &mut App,
    work: impl FnOnce(&Service) -> T + Send + 'static,
    done: impl FnOnce(Option<T>, &mut Window, &mut App) + 'static,
) {
    model.update(cx, |model, cx| {
        model.set_status(AccountStatus::SigningIn, cx)
    });
    let service = service.clone();
    window
        .spawn(cx, async move |cx| {
            let worker = service.clone();
            let result = run_blocking(move || work(&worker)).await;
            cx.update(|window, cx| {
                model.update(cx, |model, cx| model.set_status(AccountStatus::Idle, cx));
                refresh_model(&service, &model, cx);
                done(result, window, cx);
            })
            .ok();
        })
        .detach();
}

pub(crate) fn run_blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> impl Future<Output = Option<T>> {
    let (sender, receiver) = oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    async move { receiver.await.ok() }
}

fn refresh_model(service: &Service, model: &Entity<AccountsModel>, cx: &mut App) {
    let (accounts, active) = match (service.accounts(), service.current()) {
        (Ok(accounts), Ok(active)) => (accounts, active),
        (Err(error), _) | (_, Err(error)) => {
            eprintln!(
                "encore: could not read the saved accounts: {}",
                chain(&error)
            );
            return;
        }
    };
    let summaries = accounts.into_iter().map(summary).collect();
    let active = active.map(|id| SharedString::from(id.as_str().to_owned()));
    model.update(cx, |model, cx| model.set_accounts(summaries, active, cx));
}

fn summary(account: Account) -> AccountSummary {
    let name = match account.profile.name.trim().is_empty() {
        true => UNNAMED_ACCOUNT.to_owned(),
        false => account.profile.name,
    };
    AccountSummary {
        id: account.id.as_str().to_owned().into(),
        name: name.into(),
        detail: account.profile.detail.map(Into::into),
        avatar_url: account.profile.avatar_url.map(Into::into),
    }
}

fn display_name(model: &AccountsModel, id: &str) -> String {
    model
        .accounts()
        .iter()
        .find(|account| account.id.as_ref() == id)
        .map_or_else(
            || UNNAMED_ACCOUNT.to_owned(),
            |account| account.name.to_string(),
        )
}

fn expired_message(name: &str) -> String {
    match name.is_empty() {
        true => "Your session has expired. Use Add account to sign in again.".to_owned(),
        false => format!("The session for {name} has expired. Use Add account to sign in again."),
    }
}

fn signed_out_message(name: &str) -> String {
    format!("You were signed out of {name}. Use Add account to sign in again.")
}

fn sign_in_failure(error: &SignInError) -> Option<String> {
    match error {
        SignInError::Login(LoginError::Cancelled) => None,
        SignInError::Rejected => {
            Some("YouTube did not accept that sign-in. Please try again.".to_owned())
        }
        SignInError::Validation(_) => Some(shorten(
            format!(
                "Could not verify the sign-in with YouTube: {}",
                chain(error)
            ),
            MAX_MESSAGE_CHARS,
        )),
        other => Some(shorten(
            format!("Sign-in failed: {}", chain(other)),
            MAX_MESSAGE_CHARS,
        )),
    }
}

fn chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let cause_text = cause.to_string();
        if !text.contains(&cause_text) {
            text.push_str(": ");
            text.push_str(&cause_text);
        }
        source = cause.source();
    }
    text
}

fn shorten(message: String, max_chars: usize) -> String {
    match message.chars().count() > max_chars {
        true => message.chars().take(max_chars).chain(['…']).collect(),
        false => message,
    }
}

#[cfg(test)]
mod tests {
    use encore_core::AccountProfile;

    use super::*;

    fn account(name: &str, detail: Option<&str>, avatar: Option<&str>) -> Account {
        Account {
            id: AccountId::new("108123"),
            session_cookie: "supersecret".to_owned(),
            added_at_unix: 5,
            profile: AccountProfile {
                name: name.to_owned(),
                detail: detail.map(str::to_owned),
                avatar_url: avatar.map(str::to_owned),
            },
        }
    }

    #[test]
    fn a_summary_carries_the_profile_and_never_the_cookie() {
        let summary = summary(account(
            "Alice",
            Some("@alice"),
            Some("https://img.example/a.jpg"),
        ));

        assert_eq!(summary.id.as_ref(), "108123");
        assert_eq!(summary.name.as_ref(), "Alice");
        assert_eq!(summary.detail.as_deref(), Some("@alice"));
        assert_eq!(
            summary.avatar_url.as_deref(),
            Some("https://img.example/a.jpg")
        );
        assert!(!format!("{summary:?}").contains("supersecret"));
    }

    #[test]
    fn an_account_without_a_name_gets_a_readable_fallback() {
        for name in ["", "   "] {
            assert_eq!(
                summary(account(name, None, None)).name.as_ref(),
                UNNAMED_ACCOUNT
            );
        }
    }

    #[test]
    fn cancelling_the_login_window_is_not_an_error_worth_showing() {
        assert_eq!(
            sign_in_failure(&SignInError::Login(LoginError::Cancelled)),
            None
        );
    }

    #[test]
    fn real_sign_in_failures_are_explained_to_the_user() {
        let rejected = sign_in_failure(&SignInError::Rejected).unwrap();
        let offline =
            sign_in_failure(&SignInError::Validation("timed out".to_owned().into())).unwrap();
        let helper = sign_in_failure(&SignInError::Login(LoginError::Backend(
            "no window".to_owned().into(),
        )))
        .unwrap();

        assert!(rejected.contains("did not accept"));
        assert!(offline.starts_with("Could not verify the sign-in with YouTube"));
        assert!(offline.contains("timed out"));
        assert!(helper.starts_with("Sign-in failed:"));
        assert!(helper.contains("no window"));
    }

    #[test]
    fn expiry_messages_name_the_account_when_known() {
        assert!(expired_message("Alice").contains("Alice"));
        assert!(!expired_message("").contains("  "));
        assert!(signed_out_message("Alice").contains("Alice"));
    }

    #[test]
    fn a_noisy_failure_is_cut_down_to_a_readable_length() {
        let noisy = "x".repeat(5_000);

        let message =
            sign_in_failure(&SignInError::Login(LoginError::Backend(noisy.into()))).unwrap();

        assert_eq!(message.chars().count(), MAX_MESSAGE_CHARS + 1);
        assert!(message.ends_with('…'));
    }

    #[test]
    fn a_short_message_is_left_alone() {
        assert_eq!(shorten("short".to_owned(), 10), "short");
        assert_eq!(shorten("exactly10!".to_owned(), 10), "exactly10!");
    }

    #[test]
    fn a_storage_failure_shows_its_cause_not_just_internal_error() {
        let error = SignInError::Storage(encore_core::StorageError::Internal(Box::new(
            std::io::Error::other("disk is full"),
        )));

        let message = sign_in_failure(&error).unwrap();

        assert!(message.contains("internal storage error"), "{message}");
        assert!(message.contains("disk is full"), "{message}");
    }

    #[test]
    fn a_cause_already_named_in_the_message_is_not_repeated() {
        let error = LoginError::Backend("window broke".to_owned().into());

        let text = chain(&error);

        assert_eq!(text.matches("window broke").count(), 1, "{text}");
    }
}
