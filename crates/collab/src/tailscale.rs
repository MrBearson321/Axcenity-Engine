//! Tailscale, the private network Collaborate uses to reach a host from
//! anywhere (C3 of `docs/research/collaborate.md`, ADR-0021).
//!
//! Axcenity never runs a network stack of its own for this: it reads and
//! drives the Tailscale app the user installed, through its command-line
//! tool (`tailscale status --json`, `tailscale up --json`, `tailscale
//! whois`). Tailscale only gives the network path. Who may join and edit
//! is still decided by Collaborate's own keys, invites and roles: being on
//! someone's tailnet grants nothing.
//!
//! Everything that runs the tool goes through [`Runner`], so tests feed
//! recorded output instead of needing Tailscale installed.

use std::io::{BufRead, BufReader};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Where Tailscale's own download page is, for the setup wizard.
pub const DOWNLOAD_PAGE: &str = "https://tailscale.com/download";

/// The longest one command may take before it counts as not answering.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

/// Where this computer's Tailscale stands, from `tailscale status --json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// The Tailscale tool isn't installed (or isn't where it usually is).
    Missing,
    /// The tool is there but its background service doesn't answer: the
    /// app isn't running. `detail` is what the tool said.
    NotRunning { detail: String },
    /// Installed and running, but nobody has signed in on this computer.
    /// `auth_url` is the sign-in page when Tailscale already made one.
    NeedsLogin { auth_url: Option<String> },
    /// Signed in, but the tailnet's admin has to approve this computer.
    NeedsApproval,
    /// Signed in, but disconnected (switched off in the Tailscale app).
    Stopped,
    /// Connecting.
    Starting,
    /// Connected.
    Running(Tailnet),
    /// A state this version doesn't know.
    Other(String),
}

impl State {
    pub fn is_running(&self) -> bool {
        matches!(self, State::Running(_))
    }

    /// What the state means, in words for someone who has never heard of
    /// a tailnet.
    pub fn describe(&self) -> String {
        match self {
            State::Missing => "Tailscale isn't installed on this computer.".to_owned(),
            State::NotRunning { .. } => {
                "Tailscale is installed but not running. Open the Tailscale app.".to_owned()
            }
            State::NeedsLogin { .. } => {
                "Tailscale is running, but you haven't signed in yet.".to_owned()
            }
            State::NeedsApproval => "You're signed in to Tailscale, but this computer is waiting \
                                     for the network's admin to approve it."
                .to_owned(),
            State::Stopped => {
                "Tailscale is switched off. Turn it on in the Tailscale app.".to_owned()
            }
            State::Starting => "Tailscale is connecting…".to_owned(),
            State::Running(net) => match &net.login {
                Some(login) => format!("Connected to Tailscale as {login} ({}).", net.machine),
                None => format!("Connected to Tailscale ({}).", net.machine),
            },
            State::Other(state) => format!("Tailscale says \"{state}\"."),
        }
    }
}

/// This computer on its tailnet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tailnet {
    /// The machine's short name, such as `pauls-pc`.
    pub machine: String,
    /// Its full MagicDNS name, such as `pauls-pc.tail1234.ts.net` (no
    /// trailing dot), when the tailnet has MagicDNS on.
    pub dns_name: Option<String>,
    /// Its Tailscale addresses (`100.x.y.z`, and IPv6).
    pub addresses: Vec<IpAddr>,
    /// The tailnet's name, such as `paul@example.com` or `example.org`.
    pub name: Option<String>,
    /// Who is signed in, such as `paul@example.com`.
    pub login: Option<String>,
}

impl Tailnet {
    /// The `host:port` an invite should name so anyone who can reach this
    /// machine over Tailscale can join: the full MagicDNS name (the only
    /// name that works from another tailnet the machine is shared with),
    /// else the IPv4 address, else any address.
    pub fn invite_address(&self, port: u16) -> Option<String> {
        if let Some(name) = &self.dns_name {
            return Some(format!("{name}:{port}"));
        }
        let ip = self
            .addresses
            .iter()
            .find(|ip| ip.is_ipv4())
            .or_else(|| self.addresses.first())?;
        Some(SocketAddr::new(*ip, port).to_string())
    }
}

/// Reads `tailscale status --json` output.
pub fn parse_status(json: &str) -> Result<State, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("unreadable Tailscale status: {e}"))?;
    let text = |v: &serde_json::Value| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    let backend = value
        .get("BackendState")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    Ok(match backend {
        "NeedsLogin" => State::NeedsLogin {
            auth_url: value.get("AuthURL").and_then(text),
        },
        "NeedsMachineAuth" => State::NeedsApproval,
        "Stopped" => State::Stopped,
        "Starting" | "NoState" | "" => State::Starting,
        "Running" => {
            let me = value.get("Self").cloned().unwrap_or_default();
            let addresses = me
                .get("TailscaleIPs")
                .or_else(|| value.get("TailscaleIPs"))
                .and_then(|v| v.as_array())
                .map(|list| {
                    list.iter()
                        .filter_map(|ip| ip.as_str()?.parse().ok())
                        .collect()
                })
                .unwrap_or_default();
            let tailnet = value.get("CurrentTailnet");
            let magic_dns = tailnet
                .and_then(|t| t.get("MagicDNSEnabled"))
                .and_then(|v| v.as_bool())
                // Older clients have no CurrentTailnet; a DNS name ending
                // in the MagicDNS suffix means it's on.
                .unwrap_or_else(|| value.get("MagicDNSSuffix").and_then(text).is_some());
            let dns_name = me
                .get("DNSName")
                .and_then(text)
                .map(|n| n.trim_end_matches('.').to_owned())
                .filter(|n| magic_dns && !n.is_empty());
            let machine = me
                .get("HostName")
                .and_then(text)
                .or_else(|| {
                    dns_name
                        .as_deref()
                        .and_then(|n| n.split('.').next())
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| "this computer".to_owned());
            let login = me
                .get("UserID")
                .and_then(|id| {
                    let key = match id {
                        serde_json::Value::Number(n) => n.to_string(),
                        other => other.as_str()?.to_owned(),
                    };
                    value.get("User")?.get(key)?.get("LoginName").and_then(text)
                })
                .filter(|login| login != "tagged-devices");
            State::Running(Tailnet {
                machine,
                dns_name,
                addresses,
                name: tailnet.and_then(|t| t.get("Name")).and_then(text),
                login,
            })
        }
        other => State::Other(other.to_owned()),
    })
}

/// Reads the sign-in page from `tailscale up --json` output, which
/// prints JSON objects with an `AuthURL` while it waits for a sign-in.
pub fn parse_auth_url(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    value
        .get("AuthURL")?
        .as_str()
        .filter(|u| u.starts_with("https://"))
        .map(str::to_owned)
}

/// Reads who is behind a Tailscale address from `tailscale whois --json`:
/// their login, shown beside their Collaborate name (never trusted for
/// permissions).
pub fn parse_whois(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let profile = value.get("UserProfile")?;
    profile
        .get("LoginName")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty() && *s != "tagged-devices")
        .map(str::to_owned)
}

/// Whether `ip` is a Tailscale address (the `100.64.0.0/10` block, or
/// Tailscale's IPv6 prefix `fd7a:115c:a1e0::/48`).
pub fn is_tailscale_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            a == 100 && (64..128).contains(&b)
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0
        }
    }
}

/// Whether an invite's `host:port` points at a machine over Tailscale.
pub fn is_tailscale_host(host: &str) -> bool {
    let name = host.rsplit_once(':').map_or(host, |(name, _)| name);
    let name = name.trim_start_matches('[').trim_end_matches(']');
    name.ends_with(".ts.net") || name.parse().is_ok_and(is_tailscale_ip)
}

/// Runs the Tailscale tool. The real one starts a process; tests use a
/// recorded one.
pub trait Runner {
    /// Runs `tailscale <args>` and returns its standard output, or what it
    /// said went wrong.
    fn run(&self, args: &[&str]) -> Result<String, String>;
}

/// The installed Tailscale tool.
#[derive(Clone, Debug)]
pub struct Cli {
    program: PathBuf,
}

impl Cli {
    /// Finds the tool: `AXCENITY_TAILSCALE` (a path; `off` for none), then
    /// the `PATH`, then where Tailscale's installers put it.
    pub fn find() -> Option<Cli> {
        if let Some(path) = std::env::var_os("AXCENITY_TAILSCALE") {
            if path == "off" {
                return None;
            }
            let path = PathBuf::from(path);
            return path.is_file().then_some(Cli { program: path });
        }
        let name = if cfg!(windows) {
            "tailscale.exe"
        } else {
            "tailscale"
        };
        let on_path = std::env::var_os("PATH")
            .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .map(|dir| dir.join(name));
        on_path
            .chain(install_locations())
            .find(|path| path.is_file())
            .map(|program| Cli { program })
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    /// Asks Tailscale to start signing in, and returns the sign-in page to
    /// open in the browser. `tailscale up` keeps waiting for the sign-in;
    /// it's left running in the background and ends by itself when the
    /// user has signed in (the wizard watches [`state`] meanwhile).
    pub fn start_login(&self) -> Result<Option<String>, String> {
        let mut child = command(&self.program)
            .args(["up", "--json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| format!("couldn't start Tailscale: {e}"))?;
        let stdout = child.stdout.take().ok_or("Tailscale gave no output")?;
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("tailscale up".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if let Some(url) = parse_auth_url(&line) {
                        let _ = send.send(url);
                    }
                }
                // Reap it so it doesn't linger as a zombie on Unix.
                let _ = child.wait();
            })
            .map_err(|e| format!("couldn't watch Tailscale: {e}"))?;
        // Already signed in: `up` prints nothing and ends.
        Ok(receive.recv_timeout(Duration::from_secs(15)).ok())
    }
}

impl Runner for Cli {
    fn run(&self, args: &[&str]) -> Result<String, String> {
        let mut child = command(&self.program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| format!("couldn't run Tailscale: {e}"))?;
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if started.elapsed() > COMMAND_TIMEOUT => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("Tailscale didn't answer within 5 s".to_owned());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(e) => return Err(format!("couldn't run Tailscale: {e}")),
            }
        }
        let output = child
            .wait_with_output()
            .map_err(|e| format!("couldn't read Tailscale's answer: {e}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if output.status.success() || !stdout.trim().is_empty() {
            Ok(stdout)
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
        }
    }
}

/// Where this computer's Tailscale stands. `None` means the tool wasn't
/// found.
pub fn state(runner: Option<&dyn Runner>) -> State {
    let Some(runner) = runner else {
        return State::Missing;
    };
    match runner.run(&["status", "--json"]) {
        Ok(json) => parse_status(&json).unwrap_or_else(|detail| State::NotRunning { detail }),
        Err(detail) => State::NotRunning { detail },
    }
}

/// Who is signed in to Tailscale behind `address` (a member's
/// connection), if Tailscale knows.
pub fn whois(runner: &dyn Runner, address: IpAddr) -> Option<String> {
    if !is_tailscale_ip(address) {
        return None;
    }
    let json = runner
        .run(&["whois", "--json", &address.to_string()])
        .ok()?;
    parse_whois(&json)
}

/// The `host:port` invites should name: this computer's Tailscale name
/// when Tailscale is connected, else `None` (the LAN address is used).
pub fn invite_address(runner: Option<&dyn Runner>, port: u16) -> Option<String> {
    match state(runner) {
        State::Running(net) => net.invite_address(port),
        _ => None,
    }
}

/// Points `session`'s invites at this computer's Tailscale name when the
/// installed Tailscale is connected, so they work from anywhere the
/// joiner's Tailscale can reach (C3). Returns what it found, to tell the
/// person hosting. `AXCENITY_TAILSCALE=off` leaves invites on the LAN.
pub fn advertise(session: &mut crate::session::HostSession) -> State {
    if std::env::var_os("AXCENITY_TAILSCALE").is_some_and(|v| v == "off") {
        return State::Missing;
    }
    let cli = Cli::find();
    let found = state(cli.as_ref().map(|c| c as &dyn Runner));
    if let State::Running(net) = &found
        && let Some(address) = net.invite_address(session.local_address().port())
    {
        session.set_address(address);
    }
    found
}

/// Checks that the host in an invite can be reached before joining, so a
/// joiner gets a plain answer instead of a handshake that times out.
pub fn check_reachable(host: &str, timeout: Duration) -> Result<(), String> {
    let over_tailscale = is_tailscale_host(host);
    let addresses: Vec<SocketAddr> = match host.to_socket_addrs() {
        Ok(found) => found.collect(),
        Err(_) if over_tailscale => {
            return Err(format!(
                "this computer can't find `{host}`. It's a Tailscale name: make sure Tailscale \
                 is running and signed in here, and that the host's computer is shared with you \
                 (or you're on the same tailnet)."
            ));
        }
        Err(error) => return Err(format!("this computer can't find `{host}`: {error}")),
    };
    let mut last = None;
    for address in &addresses {
        match TcpStream::connect_timeout(address, timeout) {
            Ok(_) => return Ok(()),
            Err(error) => last = Some(error),
        }
    }
    let why = last.map_or_else(|| "no address".to_owned(), |e| e.to_string());
    Err(if over_tailscale {
        format!(
            "the host's computer (`{host}`) didn't answer over Tailscale ({why}). Check that \
             the host's session is still running, that Tailscale is on at both ends, and that \
             the host's computer is shared with you."
        )
    } else {
        format!(
            "the host (`{host}`) didn't answer ({why}). Check that the session is still \
             running and that you're on the same network, or use Tailscale."
        )
    })
}

pub(crate) fn command(program: &Path) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    // From a windowed program, don't flash a console window.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn install_locations() -> Vec<PathBuf> {
    if cfg!(windows) {
        let program_files = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
        vec![program_files.join("Tailscale").join("tailscale.exe")]
    } else if cfg!(target_os = "macos") {
        vec![
            // The app's own binary is also its command-line tool.
            PathBuf::from("/Applications/Tailscale.app/Contents/MacOS/Tailscale"),
            PathBuf::from("/opt/homebrew/bin/tailscale"),
            PathBuf::from("/usr/local/bin/tailscale"),
        ]
    } else {
        vec![
            PathBuf::from("/usr/bin/tailscale"),
            PathBuf::from("/usr/sbin/tailscale"),
            PathBuf::from("/usr/local/bin/tailscale"),
        ]
    }
}
