//! Tailscale state from recorded `tailscale` output (C3). The shapes follow
//! Tailscale's `ipnstate.Status` JSON; real recordings from each platform
//! replace these as they're taken.

use std::cell::RefCell;
use std::net::{IpAddr, TcpListener};
use std::time::Duration;

use crate::tailscale::*;

/// `tailscale status --json` on a connected Windows PC (trimmed: peers
/// left out).
const RUNNING: &str = r#"{
  "Version": "1.90.6-t1234",
  "TUN": true,
  "BackendState": "Running",
  "AuthURL": "",
  "TailscaleIPs": ["100.101.102.103", "fd7a:115c:a1e0::1234:5678"],
  "Self": {
    "ID": "nABC123",
    "PublicKey": "nodekey:00",
    "HostName": "Pauls-PC",
    "DNSName": "pauls-pc.tail1234.ts.net.",
    "OS": "windows",
    "UserID": 1234567890,
    "TailscaleIPs": ["100.101.102.103", "fd7a:115c:a1e0::1234:5678"],
    "Online": true
  },
  "MagicDNSSuffix": "tail1234.ts.net",
  "CurrentTailnet": {
    "Name": "paul@example.com",
    "MagicDNSSuffix": "tail1234.ts.net",
    "MagicDNSEnabled": true
  },
  "User": {
    "1234567890": {
      "ID": 1234567890,
      "LoginName": "paul@example.com",
      "DisplayName": "Paul"
    }
  },
  "Health": []
}"#;

struct Recorded {
    answers: Vec<(&'static str, Result<String, String>)>,
    asked: RefCell<Vec<String>>,
}

impl Recorded {
    fn new(answers: Vec<(&'static str, Result<String, String>)>) -> Recorded {
        Recorded {
            answers,
            asked: RefCell::new(Vec::new()),
        }
    }
}

impl Runner for Recorded {
    fn run(&self, args: &[&str]) -> Result<String, String> {
        let line = args.join(" ");
        self.asked.borrow_mut().push(line.clone());
        self.answers
            .iter()
            .find(|(start, _)| line.starts_with(start))
            .map(|(_, answer)| answer.clone())
            .unwrap_or_else(|| Err(format!("unexpected: tailscale {line}")))
    }
}

fn status(json: &str) -> Recorded {
    Recorded::new(vec![("status --json", Ok(json.to_owned()))])
}

#[test]
fn a_connected_computer_names_itself_by_its_full_tailscale_name() {
    let State::Running(net) = parse_status(RUNNING).unwrap() else {
        panic!("running")
    };
    assert_eq!(net.machine, "Pauls-PC");
    assert_eq!(net.dns_name.as_deref(), Some("pauls-pc.tail1234.ts.net"));
    assert_eq!(net.login.as_deref(), Some("paul@example.com"));
    assert_eq!(net.name.as_deref(), Some("paul@example.com"));
    assert_eq!(net.addresses.len(), 2);
    assert_eq!(
        net.invite_address(47810).as_deref(),
        Some("pauls-pc.tail1234.ts.net:47810"),
        "the full name is the one a shared machine is reached by"
    );
    let runner = status(RUNNING);
    assert_eq!(
        invite_address(Some(&runner), 47810).as_deref(),
        Some("pauls-pc.tail1234.ts.net:47810")
    );
}

#[test]
fn without_magic_dns_invites_name_the_ipv4_address() {
    let json = RUNNING.replace(r#""MagicDNSEnabled": true"#, r#""MagicDNSEnabled": false"#);
    let State::Running(net) = parse_status(&json).unwrap() else {
        panic!("running")
    };
    assert_eq!(net.dns_name, None);
    assert_eq!(net.machine, "Pauls-PC");
    assert_eq!(
        net.invite_address(47810).as_deref(),
        Some("100.101.102.103:47810")
    );
}

#[test]
fn each_backend_state_becomes_a_step_of_setup() {
    let cases = [
        (
            r#"{"BackendState":"NeedsLogin","AuthURL":"https://login.tailscale.com/a/abc123"}"#,
            State::NeedsLogin {
                auth_url: Some("https://login.tailscale.com/a/abc123".to_owned()),
            },
        ),
        (
            r#"{"BackendState":"NeedsLogin","AuthURL":""}"#,
            State::NeedsLogin { auth_url: None },
        ),
        (
            r#"{"BackendState":"NeedsMachineAuth"}"#,
            State::NeedsApproval,
        ),
        (r#"{"BackendState":"Stopped"}"#, State::Stopped),
        (r#"{"BackendState":"Starting"}"#, State::Starting),
        (r#"{"BackendState":"NoState"}"#, State::Starting),
        (
            r#"{"BackendState":"Wobbly"}"#,
            State::Other("Wobbly".to_owned()),
        ),
    ];
    for (json, expected) in cases {
        assert_eq!(parse_status(json).unwrap(), expected, "{json}");
        assert!(!expected.describe().is_empty());
    }
    assert!(parse_status("not json").is_err());
}

#[test]
fn missing_or_silent_tailscale_is_told_apart() {
    assert_eq!(state(None), State::Missing);
    let silent = Recorded::new(vec![(
        "status --json",
        Err("failed to connect to local tailscaled; it doesn't appear to be running".to_owned()),
    )]);
    let State::NotRunning { detail } = state(Some(&silent)) else {
        panic!("not running")
    };
    assert!(detail.contains("tailscaled"), "{detail}");
    assert_eq!(
        invite_address(Some(&silent), 47810),
        None,
        "falls back to the LAN"
    );
    assert_eq!(invite_address(None, 47810), None);
}

#[test]
fn tagged_machines_have_no_login() {
    let json = RUNNING.replace(
        "paul@example.com\",\n      \"DisplayName",
        "tagged-devices\",\n      \"DisplayName",
    );
    let State::Running(net) = parse_status(&json).unwrap() else {
        panic!("running")
    };
    assert_eq!(net.login, None);
    assert!(net.dns_name.is_some());
}

#[test]
fn the_sign_in_page_comes_from_tailscale_up() {
    assert_eq!(
        parse_auth_url(
            r#"{"AuthURL":"https://login.tailscale.com/a/xyz","QR":"","BackendState":"NeedsLogin"}"#
        )
        .as_deref(),
        Some("https://login.tailscale.com/a/xyz")
    );
    assert_eq!(parse_auth_url(r#"{"BackendState":"Running"}"#), None);
    assert_eq!(parse_auth_url(r#"{"AuthURL":"javascript:alert(1)"}"#), None);
    assert_eq!(parse_auth_url("Success."), None);
}

#[test]
fn whois_names_a_member_only_for_tailscale_addresses() {
    let runner = Recorded::new(vec![(
        "whois --json 100.70.1.2",
        Ok(r#"{"Node":{"Name":"sams-mac.tail9876.ts.net."},"UserProfile":{"LoginName":"sam@example.org","DisplayName":"Sam"}}"#.to_owned()),
    )]);
    let sam: IpAddr = "100.70.1.2".parse().unwrap();
    assert_eq!(whois(&runner, sam).as_deref(), Some("sam@example.org"));
    let lan: IpAddr = "192.168.1.20".parse().unwrap();
    assert_eq!(whois(&runner, lan), None);
    assert_eq!(
        runner.asked.borrow().as_slice(),
        ["whois --json 100.70.1.2"],
        "a LAN address isn't asked about"
    );
}

#[test]
fn tailscale_addresses_and_names_are_recognized() {
    for host in [
        "pauls-pc.tail1234.ts.net:47810",
        "100.64.0.1:47810",
        "100.127.255.254:47810",
        "[fd7a:115c:a1e0::1]:47810",
    ] {
        assert!(is_tailscale_host(host), "{host}");
    }
    for host in [
        "192.168.1.20:47810",
        "100.128.0.1:47810",
        "100.63.255.255:47810",
        "example.com:47810",
        "[::1]:47810",
    ] {
        assert!(!is_tailscale_host(host), "{host}");
    }
}

#[test]
fn reachability_is_checked_before_joining() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let open = listener.local_addr().unwrap().to_string();
    assert_eq!(check_reachable(&open, Duration::from_secs(2)), Ok(()));
    drop(listener);
    let error = check_reachable(&open, Duration::from_secs(2)).unwrap_err();
    assert!(error.contains("didn't answer"), "{error}");
    assert!(!error.contains("Tailscale is on"), "a LAN host: {error}");
    // An unknown Tailscale name says what to check, in plain words.
    let error = check_reachable(
        "no-such-machine.tail0000.ts.net:47810",
        Duration::from_millis(500),
    )
    .unwrap_err();
    assert!(error.contains("Tailscale"), "{error}");
    assert!(error.contains("shared with you"), "{error}");
}
