use std::io::BufRead as _;
use std::path::PathBuf;

use anyhow::{Context as _, bail};
use clap::{Args, Parser, Subcommand};

const LOGIN_URL: &str =
    "https://accounts.google.com/ServiceLogin?service=youtube&continue=https://music.youtube.com/";
const MUSIC_URL: &str = "https://music.youtube.com/";

#[derive(Parser)]
#[command(
    name = "encore-login",
    version,
    about = "Signs in to YouTube Music for Encore"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Open a window to sign in and print the cookie header")]
    Login {
        #[arg(
            long,
            value_name = "PATH",
            help = "Directory to save the session in; without it, login runs in a private window"
        )]
        data_dir: Option<PathBuf>,
    },
    #[command(about = "Print a fresher cookie header for the one read from stdin, then exit")]
    Refresh(Profile),
    #[command(
        name = "keep-alive",
        about = "Stay running in a hidden window, printing each new cookie header, until stdin closes"
    )]
    KeepAlive(Profile),
}

#[derive(Args)]
struct Profile {
    #[arg(
        long,
        value_name = "PATH",
        help = "Directory holding the saved login session"
    )]
    data_dir: PathBuf,
}

pub enum Mode {
    Login { data_dir: Option<PathBuf> },
    Refresh { data_dir: PathBuf, current: String },
    KeepAlive { data_dir: PathBuf, current: String },
}

impl Mode {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_command(Cli::parse().command)
    }

    fn from_command(command: Command) -> anyhow::Result<Self> {
        Ok(match command {
            Command::Login { data_dir } => Self::Login { data_dir },
            Command::Refresh(Profile { data_dir }) => Self::Refresh {
                data_dir,
                current: read_current_cookie("refresh")?,
            },
            Command::KeepAlive(Profile { data_dir }) => Self::KeepAlive {
                data_dir,
                current: read_current_cookie("keep-alive")?,
            },
        })
    }

    pub fn url(&self) -> &'static str {
        match self {
            Self::Login { .. } => LOGIN_URL,
            Self::Refresh { .. } | Self::KeepAlive { .. } => MUSIC_URL,
        }
    }

    pub fn visible(&self) -> bool {
        matches!(self, Self::Login { .. })
    }

    pub fn data_dir(&self) -> Option<&PathBuf> {
        match self {
            Self::Login { data_dir } => data_dir.as_ref(),
            Self::Refresh { data_dir, .. } | Self::KeepAlive { data_dir, .. } => Some(data_dir),
        }
    }
}

fn read_current_cookie(command: &str) -> anyhow::Result<String> {
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .context("could not read the current cookie from stdin")?;
    let current = line.trim().to_owned();
    if current.is_empty() {
        bail!("{command} expects the current cookie header on stdin");
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory as _;
    use encore_auth::protocol::{
        COMMAND_KEEP_ALIVE, COMMAND_LOGIN, COMMAND_REFRESH, FLAG_DATA_DIR,
    };

    use super::*;

    fn parse(args: &[&str]) -> Result<Command, clap::Error> {
        Cli::try_parse_from(std::iter::once("encore-login").chain(args.iter().copied()))
            .map(|cli| cli.command)
    }

    #[test]
    fn the_commands_and_flag_match_the_protocol_the_client_speaks() {
        let cli = Cli::command();

        for name in [COMMAND_LOGIN, COMMAND_REFRESH, COMMAND_KEEP_ALIVE] {
            let command = cli
                .find_subcommand(name)
                .unwrap_or_else(|| panic!("missing command {name}"));
            let flags: Vec<_> = command
                .get_arguments()
                .filter_map(|arg| arg.get_long())
                .map(|long| format!("--{long}"))
                .collect();
            assert!(flags.iter().any(|flag| flag == FLAG_DATA_DIR), "{name}");
        }
    }

    #[test]
    fn a_command_is_required() {
        assert!(parse(&[]).is_err());
    }

    #[test]
    fn login_may_run_private_or_with_a_saved_profile() {
        let private = parse(&["login"]).unwrap();
        let saved = parse(&["login", "--data-dir", "/some/dir"]).unwrap();

        assert!(matches!(private, Command::Login { data_dir: None }));
        let Command::Login { data_dir } = saved else {
            panic!("expected the login command");
        };
        assert_eq!(data_dir, Some(PathBuf::from("/some/dir")));
    }

    #[test]
    fn refresh_and_keep_alive_need_a_data_dir() {
        assert!(parse(&["refresh"]).is_err());
        assert!(parse(&["keep-alive"]).is_err());
        assert!(parse(&["refresh", "--data-dir", "/d"]).is_ok());
        assert!(parse(&["keep-alive", "--data-dir", "/d"]).is_ok());
    }

    #[test]
    fn unknown_commands_and_flags_are_rejected() {
        assert!(parse(&["bogus"]).is_err());
        assert!(parse(&["login", "--bogus"]).is_err());
    }
}
