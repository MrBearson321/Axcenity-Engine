//! `axcenity-collab <project-folder> [--port <port>] [--data <folder>] [--address <host:port>] [--no-tailscale]`:
//! Collaborate's self-hosted server (TC7). It hosts the project for
//! editors to join with invite links, saves it every 30 seconds with
//! changes, and takes admin commands typed into its terminal (`help`
//! lists them). Stop it with `quit` (Ctrl+C works too: the next start
//! brings back what wasn't saved).

use std::io::BufRead;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use axcenity_collab_server::{Options, Server};

const USAGE: &str = "usage: axcenity-collab <project-folder> [--port <port>] [--data <folder>] [--address <host:port>] [--no-tailscale]\n\
                     example: cargo run -p axcenity-collab-server -- examples/hello-part";

fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Options> {
    let mut project = None;
    let mut port = None;
    let mut data = None;
    let mut address = None;
    let mut tailscale = true;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let mut value = |name: &str| {
            arguments
                .next()
                .with_context(|| format!("{name} needs a value\n{USAGE}"))
        };
        match argument.as_str() {
            "--port" => {
                let text = value("--port")?;
                port = Some(
                    text.parse()
                        .with_context(|| format!("`{text}` isn't a port"))?,
                );
            }
            "--data" => data = Some(PathBuf::from(value("--data")?)),
            "--address" => address = Some(value("--address")?),
            "--no-tailscale" => tailscale = false,
            "--help" | "-h" => bail!("{USAGE}"),
            other if other.starts_with("--") => bail!("unknown option `{other}`\n{USAGE}"),
            other if project.is_none() => project = Some(PathBuf::from(other)),
            other => bail!("unexpected `{other}`\n{USAGE}"),
        }
    }
    let project = project.with_context(|| format!("which project?\n{USAGE}"))?;
    if !axcenity_collab_server::is_project(&project) {
        bail!("`{}` isn't an Axcenity project folder", project.display());
    }
    let mut options = Options::new(project)?;
    if let Some(port) = port {
        options.port = port;
    }
    if let Some(data) = data {
        options.data = data;
    }
    options.address = address;
    options.tailscale = tailscale;
    Ok(options)
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let options = parse(std::env::args().skip(1))?;
    let (mut server, said) = Server::start(&options)?;
    for line in said {
        println!("{line}");
    }
    println!("Type `help` for the commands.");
    // The terminal's lines, read on their own thread. Without a terminal
    // (as a service) the server keeps running.
    let (lines_in, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if lines_in.send(line).is_err() {
                break;
            }
        }
    });
    loop {
        match lines.recv_timeout(Duration::from_millis(250)) {
            Ok(line) => {
                let reply = server.command(&line);
                if !reply.text.is_empty() {
                    println!("{}", reply.text);
                }
                if reply.quit {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            // No terminal: no more commands, but the session goes on.
            Err(RecvTimeoutError::Disconnected) => {
                std::thread::sleep(Duration::from_millis(250));
            }
        }
        if let Some(text) = server.tick() {
            println!("{text}");
        }
    }
    server.stop()?;
    println!("Saved. The session has ended.");
    Ok(())
}
