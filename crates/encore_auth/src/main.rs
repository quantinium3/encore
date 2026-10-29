mod cli;
mod cookies;
mod keepalive;
mod poll;

use std::io::{BufRead as _, Write as _};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use cli::Mode;
use encore_auth::protocol::{EXIT_CANCELLED, EXIT_NOT_SIGNED_IN, EXIT_TIMED_OUT};
use keepalive::{CHECK_INTERVAL, Check, KeepAlive, RELOAD_INTERVAL, SETTLE_AFTER_LOAD};
use poll::{POLL_INTERVAL, Poller, Step};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use tao::window::{Window, WindowBuilder};
use url::Url;
use wry::{PageLoadEvent, WebContext, WebView, WebViewBuilder};

const YOUTUBE_MUSIC_HOST: &str = "music.youtube.com";

const LOGIN_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
     AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4.1 Safari/605.1.15";

const REFRESH_TIMEOUT: Duration = Duration::from_secs(45);

enum UserEvent {
    ReachedYoutubeMusic,
    ParentGone,
}

fn main() -> anyhow::Result<()> {
    let mode = Mode::from_env()?;

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title("Sign in to YouTube Music")
        .with_visible(mode.visible())
        .build(&event_loop)
        .context("failed to create the login window")?;

    let mut context = mode
        .data_dir()
        .map(|dir| WebContext::new(Some(dir.clone())));
    let proxy = event_loop.create_proxy();
    let webview = build_webview(&window, &mode, context.as_mut(), proxy.clone())?;

    match mode {
        Mode::Login { .. } => run_once(event_loop, webview, Poller::login(), None),
        Mode::Refresh { current, .. } => {
            let deadline = Instant::now() + REFRESH_TIMEOUT;
            run_once(
                event_loop,
                webview,
                Poller::refresh(current),
                Some(deadline),
            )
        }
        Mode::KeepAlive { current, .. } => {
            watch_parent(proxy);
            run_keep_alive(event_loop, webview, KeepAlive::new(current))
        }
    }
}

fn run_once(
    event_loop: EventLoop<UserEvent>,
    webview: WebView,
    mut poller: Poller,
    deadline: Option<Instant>,
) -> ! {
    let mut next_tick = None;

    event_loop.run(move |event, _, control_flow| {
        let now = Instant::now();
        let step = match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => Some(Step::Exit(EXIT_CANCELLED)),
            Event::UserEvent(UserEvent::ReachedYoutubeMusic) if poller.start() => {
                Some(tick(&webview, &mut poller))
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if deadline.is_some_and(|deadline| now >= deadline) {
                    Some(Step::Exit(EXIT_TIMED_OUT))
                } else if poller.is_active() && next_tick.is_some_and(|at| now >= at) {
                    Some(tick(&webview, &mut poller))
                } else {
                    None
                }
            }
            _ => None,
        };

        match step {
            Some(Step::Emit(header)) => {
                println!("{header}");
                *control_flow = ControlFlow::Exit;
                return;
            }
            Some(Step::Exit(code)) => {
                *control_flow = ControlFlow::ExitWithCode(code);
                return;
            }
            Some(Step::Retry) => next_tick = Some(now + POLL_INTERVAL),
            Some(Step::GiveUp) | None => {}
        }

        let pending_tick = next_tick.filter(|_| poller.is_active());
        *control_flow = wake_at(pending_tick, deadline);
    });
}

fn run_keep_alive(
    event_loop: EventLoop<UserEvent>,
    webview: WebView,
    mut keep_alive: KeepAlive,
) -> ! {
    let started = Instant::now();
    let mut next_check = started + CHECK_INTERVAL;
    let mut next_reload = started + RELOAD_INTERVAL;

    event_loop.run(move |event, _, control_flow| {
        let now = Instant::now();
        match event {
            Event::UserEvent(UserEvent::ParentGone) => {
                *control_flow = ControlFlow::Exit;
                return;
            }
            Event::UserEvent(UserEvent::ReachedYoutubeMusic) => {
                next_check = next_check.min(now + SETTLE_AFTER_LOAD);
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if now >= next_reload {
                    next_reload = now + RELOAD_INTERVAL;
                    let _ = webview.reload();
                }
                if now >= next_check {
                    next_check = now + CHECK_INTERVAL;
                    match keep_alive.check(read_header(&webview)) {
                        Check::Nothing => {}
                        Check::Emit(header) => {
                            if writeln!(std::io::stdout().lock(), "{header}").is_err() {
                                *control_flow = ControlFlow::Exit;
                                return;
                            }
                        }
                        Check::SignedOut => {
                            *control_flow = ControlFlow::ExitWithCode(EXIT_NOT_SIGNED_IN);
                            return;
                        }
                    }
                }
            }
            _ => {}
        }

        *control_flow = ControlFlow::WaitUntil(next_check.min(next_reload));
    });
}

fn watch_parent(proxy: EventLoopProxy<UserEvent>) {
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut ignored = String::new();
        while matches!(stdin.lock().read_line(&mut ignored), Ok(read) if read > 0) {
            ignored.clear();
        }
        let _ = proxy.send_event(UserEvent::ParentGone);
    });
}

fn build_webview(
    window: &Window,
    mode: &Mode,
    context: Option<&mut WebContext>,
    proxy: EventLoopProxy<UserEvent>,
) -> anyhow::Result<WebView> {
    let builder = match context {
        Some(context) => WebViewBuilder::new_with_web_context(context),
        None => WebViewBuilder::new().with_incognito(true),
    };
    let builder = builder
        .with_url(mode.url())
        .with_user_agent(LOGIN_USER_AGENT)
        .with_on_page_load_handler(move |event, url| {
            let finished = matches!(event, PageLoadEvent::Finished);
            if finished && is_youtube_music(&url) {
                let _ = proxy.send_event(UserEvent::ReachedYoutubeMusic);
            }
        });

    attach(builder, window)
}

#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
fn attach<'a>(builder: WebViewBuilder<'a>, window: &'a Window) -> anyhow::Result<WebView> {
    use tao::platform::unix::WindowExtUnix as _;
    use wry::WebViewBuilderExtUnix as _;

    let vbox = window
        .default_vbox()
        .context("window has no GTK container")?;
    builder
        .build_gtk(vbox)
        .context("failed to create the webview")
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
)))]
fn attach<'a>(builder: WebViewBuilder<'a>, window: &'a Window) -> anyhow::Result<WebView> {
    builder
        .build(window)
        .context("failed to create the webview")
}

fn read_header(webview: &WebView) -> String {
    let cookies = webview.cookies().unwrap_or_default();
    cookies::youtube_cookie_header(cookies)
}

fn tick(webview: &WebView, poller: &mut Poller) -> Step {
    poller.tick(read_header(webview))
}

fn wake_at(next_tick: Option<Instant>, deadline: Option<Instant>) -> ControlFlow {
    [next_tick, deadline]
        .into_iter()
        .flatten()
        .min()
        .map_or(ControlFlow::Wait, ControlFlow::WaitUntil)
}

fn is_youtube_music(url: &str) -> bool {
    Url::parse(url).is_ok_and(|url| url.host_str() == Some(YOUTUBE_MUSIC_HOST))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_youtube_music_host_counts() {
        assert!(is_youtube_music("https://music.youtube.com/"));
        assert!(is_youtube_music("https://music.youtube.com?x=1"));
        assert!(!is_youtube_music(
            "https://accounts.google.com/CheckCookie?continue=https://music.youtube.com/"
        ));
        assert!(!is_youtube_music("https://music.youtube.com.evil.com/"));
        assert!(!is_youtube_music("not a url"));
    }

    #[test]
    fn wake_at_picks_the_earliest_time() {
        let soon = Instant::now();
        let later = soon + Duration::from_secs(1);

        assert!(matches!(wake_at(None, None), ControlFlow::Wait));
        assert!(
            matches!(wake_at(Some(later), Some(soon)), ControlFlow::WaitUntil(at) if at == soon)
        );
        assert!(matches!(wake_at(Some(soon), None), ControlFlow::WaitUntil(at) if at == soon));
    }
}
