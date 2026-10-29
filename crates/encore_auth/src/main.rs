#![windows_subsystem = "windows"]

use std::io::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use encore_auth::{EXIT_CANCELLED, EXIT_SIGNED_OUT};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::{Window, WindowBuilder};
use wry::{WebContext, WebView, WebViewBuilder};

const LOGIN_URL: &str =
    "https://accounts.google.com/ServiceLogin?service=youtube&continue=https://music.youtube.com/";
const MUSIC_URL: &str = "https://music.youtube.com/";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
     AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4.1 Safari/605.1.15";

const LOGIN_POLL: Duration = Duration::from_millis(500);
const FIRST_CHECK: Duration = Duration::from_secs(10);
const CHECK_INTERVAL: Duration = Duration::from_secs(30);
const RELOAD_INTERVAL: Duration = Duration::from_secs(30 * 60);
const MISSES_BEFORE_SIGNED_OUT: u32 = 3;

#[derive(Parser)]
#[command(name = "encore-login", about = "Signs in to YouTube Music for Encore")]
struct Cli {
    #[arg(long, value_name = "PATH", help = "Directory holding the login session")]
    data_dir: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Open a window to sign in and print the cookie header")]
    Login,
    #[command(about = "Run hidden, printing each new cookie header, until stdin closes")]
    KeepAlive,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let visible = matches!(cli.command, Command::Login);

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Sign in to YouTube Music")
        .with_visible(visible)
        .build(&event_loop)
        .context("could not create the login window")?;

    let mut context = WebContext::new(Some(cli.data_dir));
    let builder = WebViewBuilder::new_with_web_context(&mut context)
        .with_url(if visible { LOGIN_URL } else { MUSIC_URL })
        .with_user_agent(USER_AGENT);
    let webview = attach(builder, &window)?;

    match cli.command {
        Command::Login => login(event_loop, webview),
        Command::KeepAlive => keep_alive(event_loop, webview),
    }
}

fn login(event_loop: EventLoop<()>, webview: WebView) -> ! {
    let mut next_poll = Instant::now();

    event_loop.run(move |event, _, control_flow| {
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::ExitWithCode(EXIT_CANCELLED);
            return;
        }

        if Instant::now() >= next_poll {
            next_poll = Instant::now() + LOGIN_POLL;
            let on_music = webview.url().is_ok_and(|url| url.starts_with(MUSIC_URL));
            if on_music && let Some(cookie) = session_cookie(&webview) {
                println!("{cookie}");
                *control_flow = ControlFlow::Exit;
                return;
            }
        }

        *control_flow = ControlFlow::WaitUntil(next_poll);
    })
}

fn keep_alive(event_loop: EventLoop<()>, webview: WebView) -> ! {
    std::thread::spawn(|| {
        let _ = std::io::copy(&mut std::io::stdin(), &mut std::io::sink());
        std::process::exit(0);
    });

    let mut last = String::new();
    let mut misses = 0;
    let mut next_check = Instant::now() + FIRST_CHECK;
    let mut next_reload = Instant::now() + RELOAD_INTERVAL;

    event_loop.run(move |_, _, control_flow| {
        let now = Instant::now();

        if now >= next_reload {
            next_reload = now + RELOAD_INTERVAL;
            let _ = webview.reload();
        }

        if now >= next_check {
            next_check = now + CHECK_INTERVAL;
            match session_cookie(&webview) {
                None => misses += 1,
                Some(cookie) => {
                    misses = 0;
                    if cookie != last {
                        let _ = writeln!(std::io::stdout(), "{cookie}");
                        last = cookie;
                    }
                }
            }
            if misses == MISSES_BEFORE_SIGNED_OUT {
                *control_flow = ControlFlow::ExitWithCode(EXIT_SIGNED_OUT);
                return;
            }
        }

        *control_flow = ControlFlow::WaitUntil(next_check.min(next_reload));
    })
}

fn session_cookie(webview: &WebView) -> Option<String> {
    let cookies = webview.cookies_for_url(MUSIC_URL).ok()?;
    if !cookies.iter().any(|cookie| cookie.name() == "SAPISID") {
        return None;
    }
    let mut pairs: Vec<_> = cookies
        .iter()
        .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
        .collect();
    pairs.sort();
    Some(pairs.join("; "))
}

#[cfg(target_os = "linux")]
fn attach<'a>(builder: WebViewBuilder<'a>, window: &'a Window) -> anyhow::Result<WebView> {
    use tao::platform::unix::WindowExtUnix as _;
    use wry::WebViewBuilderExtUnix as _;

    let vbox = window
        .default_vbox()
        .context("the window has no GTK container")?;
    builder
        .build_gtk(vbox)
        .context("could not create the webview")
}

#[cfg(not(target_os = "linux"))]
fn attach<'a>(builder: WebViewBuilder<'a>, window: &'a Window) -> anyhow::Result<WebView> {
    builder
        .build(window)
        .context("could not create the webview")
}
