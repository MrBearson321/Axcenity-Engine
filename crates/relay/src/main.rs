//! `axcenity-relay [--port <port>] [--max-games <n>] [--max-players <n>]`:
//! runs a relay until stopped (Ctrl+C), printing what happens.

use std::ffi::OsString;
use std::time::Duration;

use anyhow::{Result, bail};
use axcenity_relay::{Relay, RelaySettings};

const USAGE: &str = "usage: axcenity-relay [--port <port>] [--max-games <n>] [--max-players <n>]\n\
                     Hosts started with --relay <this computer's address>:<port> get a join code; \
                     players join with the code. Open TCP and UDP on the port (47810 unless given).";

fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<RelaySettings> {
    let mut settings = RelaySettings::default();
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let text = argument.to_string_lossy().into_owned();
        let mut number = |what: &str| -> Result<usize> {
            let value = arguments.next().map(|v| v.to_string_lossy().into_owned());
            match value.as_deref().map(str::parse::<usize>) {
                Some(Ok(n)) => Ok(n),
                _ => bail!("{what} takes a number\n\n{USAGE}"),
            }
        };
        match text.as_str() {
            "--port" => {
                let port = number("--port")?;
                if port > u16::MAX as usize {
                    bail!("--port takes a number from 0 to 65535\n\n{USAGE}");
                }
                settings.port = port as u16;
            }
            "--max-games" => settings.max_games = number("--max-games")?.max(1),
            "--max-players" => settings.max_players = number("--max-players")?.max(1),
            "--help" | "-h" => bail!("{USAGE}"),
            _ => bail!("unknown option `{text}`\n\n{USAGE}"),
        }
    }
    Ok(settings)
}

fn main() -> Result<()> {
    let settings = parse(std::env::args_os().skip(1))?;
    let relay = match Relay::start(settings.clone()) {
        Ok(relay) => relay,
        Err(err) => bail!("can't start the relay on port {}: {err}", settings.port),
    };
    println!(
        "Relay on TCP and UDP port {}, for {} games of up to {} players.",
        relay.port(),
        settings.max_games,
        settings.max_players
    );
    loop {
        for line in relay.take_log() {
            println!("{line}");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_take_numbers() {
        let parse = |a: &[&str]| parse(a.iter().map(OsString::from));
        let settings = parse(&["--port", "0", "--max-games", "3"]).unwrap();
        assert_eq!((settings.port, settings.max_games), (0, 3));
        assert!(parse(&["--port", "70000"]).is_err());
        assert!(parse(&["--max-players"]).is_err());
        assert!(parse(&["--colour"]).is_err());
    }
}
