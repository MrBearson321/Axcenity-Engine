//! The session going on when its host leaves or is lost (Collaborate 2.0,
//! P4; `docs/research/collaborate-persistent.md` §4.3–4.4, ADR-0022).
//!
//! - A member who may host sends its **card** (this computer's Tailscale
//!   name or LAN address, the usual port, and its host certificate) when
//!   it joins; the host ranks the cards into the successor list everyone
//!   holds.
//! - **The host leaves** (Leave, opening another project, quitting) while
//!   others are connected: it hands the session to the first successor
//!   instead of ending it. The successor waits until it has every
//!   transaction, then hosts from its own copy; the others go to it.
//! - **The host is lost** without a word: after the grace period every
//!   member picks the same successor from the list it last heard
//!   ([`election::after_loss`]); the successor hosts, recovering what it
//!   missed from the others, and everyone goes there.

use std::path::Path;
use std::sync::Arc;

use axcenity_collab::access::Access;
use axcenity_collab::election::{self, Choice};
use axcenity_collab::hosted::{members_file, session_world};
use axcenity_collab::project::HostCard;
use axcenity_collab::session::{HostSession, Remote};
use axcenity_collab::socket::{DEFAULT_PORT, HostCert};
use axcenity_collab::{Client, FolderStore, Host};

use super::team::Link;
use super::*;

/// This computer's address for others to reach it on `port`: its
/// Tailscale name when Tailscale runs, else its LAN address.
fn my_address(port: u16) -> Option<String> {
    let off = std::env::var_os("AXCENITY_TAILSCALE").is_some_and(|v| v == "off");
    if !off {
        let cli = axcenity_collab::tailscale::Cli::find();
        let state = axcenity_collab::tailscale::state(
            cli.as_ref()
                .map(|c| c as &dyn axcenity_collab::tailscale::Runner),
        );
        if let axcenity_collab::tailscale::State::Running(net) = state
            && let Some(address) = net.invite_address(port)
        {
            return Some(address);
        }
    }
    axcenity_collab::session::lan_address()
        .map(|ip| std::net::SocketAddr::new(ip, port).to_string())
}

impl EditorApp {
    /// Each frame in a network session: sends this computer's card once
    /// joined, and follows the host leaving or being lost.
    pub(super) fn team_handoff_frame(&mut self) {
        self.send_host_card();
        self.follow_handoff();
    }

    /// A member who may host says how to reach it (once per session).
    fn send_host_card(&mut self) {
        let Some(team) = self.team.as_mut() else {
            return;
        };
        if team.card_sent
            || team.hosting().is_some()
            || !team.lets_others_host
            || self.team_panel.no_host_offer
        {
            return;
        }
        let Some(data) = team.data.clone() else {
            return;
        };
        let may_host = team
            .client()
            .is_some_and(|c| c.is_joined() && c.role().may_host());
        if !may_host || !matches!(team.link, Link::Net { .. }) {
            return;
        }
        team.card_sent = true;
        let cert = match HostCert::load_or_make(&data.join("host-certificate.json")) {
            Ok(cert) => cert,
            Err(error) => {
                log::warn!(
                    "Collaborate: no host certificate, so this computer can't take over: {error}"
                );
                return;
            }
        };
        let Some(address) = team
            .card_address
            .clone()
            .or_else(|| my_address(DEFAULT_PORT))
        else {
            return;
        };
        team.card_address = Some(address.clone());
        if let Some(client) = team.client_mut() {
            client.set_card(address, cert.fingerprint());
        }
    }

    /// The host said it's leaving, or went quiet: go where the session
    /// goes on.
    fn follow_handoff(&mut self) {
        let Some(team) = self.team.as_mut() else {
            return;
        };
        if team.hosting().is_some() || !matches!(team.link, Link::Net { .. }) {
            return;
        }
        let Some(client) = team.client() else {
            return;
        };
        let me = client.member();
        let online = client.is_online();
        let seq = client.seq();
        let handoff = client.handoff().cloned();
        let successors = client.successors().to_vec();
        if online {
            team.lost_at = None;
        }
        // The host said who's next.
        if let Some(handoff) = handoff
            && team.followed_handoff != Some(handoff.last_seq)
        {
            match handoff.to {
                Some(card) if card.member == Some(me) => {
                    if seq >= handoff.last_seq {
                        team.followed_handoff = Some(handoff.last_seq);
                        self.take_over(false);
                    }
                }
                Some(card) => {
                    team.followed_handoff = Some(handoff.last_seq);
                    self.output
                        .info("Collaborate: the host left; joining the new host");
                    self.move_to_host(&card);
                }
                None => {
                    team.followed_handoff = Some(handoff.last_seq);
                    team.problem = Some("The host left and nobody else can host".to_owned());
                    self.output.warning(
                        "Collaborate: the host left and nobody else can host right now. Your \
                         copy stays open and your edits are kept; rejoin when someone hosts.",
                    );
                }
            }
            return;
        }
        // The host went quiet: wait out the grace period, then go down
        // the successor list.
        let Some(lost_at) = team.lost_at else {
            return;
        };
        match election::after_loss(lost_at.elapsed(), team.timing, &successors, me) {
            Choice::WaitForHost => {}
            Choice::Host => self.take_over(true),
            Choice::Join(index) => {
                let card = successors[index].clone();
                if team.address() != card.address {
                    self.output.info(format!(
                        "Collaborate: the host is gone; trying the next host ({})",
                        card.address
                    ));
                    self.move_to_host(&card);
                }
            }
            Choice::Alone => {
                if team.problem.as_deref() != Some(ALONE) {
                    team.problem = Some(ALONE.to_owned());
                    self.output.warning(format!("Collaborate: {ALONE}"));
                }
            }
        }
    }

    /// Joins the host `card` names, by key (no invite).
    fn move_to_host(&mut self, card: &HostCard) {
        let Some(team) = self.team.as_mut() else {
            return;
        };
        let Some(data) = team.data.clone() else {
            return;
        };
        let identity = match super::team::load_identity(&data) {
            Ok(identity) => identity,
            Err(error) => {
                self.output
                    .error(format!("Collaborate: your identity: {error}"));
                return;
            }
        };
        team.link = Link::Net {
            remote: Box::new(Remote::join(
                card.address.clone(),
                card.cert,
                None,
                Arc::new(identity),
            )),
            hosting: None,
        };
        team.set_address(card.address.clone());
        team.host_cert = Some(card.cert);
        if let Some(client) = team.client_mut() {
            client.disconnected();
        }
    }

    /// Hosts the session from this copy: the World without this editor's
    /// unconfirmed edits, the members list it was sent, the project's ID,
    /// its folder of the session's files, on its card's port. `recover`:
    /// the old host was lost, so what others have and this copy doesn't is
    /// gathered first.
    pub(super) fn take_over(&mut self, recover: bool) {
        if let Err(error) = self.try_take_over(recover) {
            self.output
                .error(format!("Collaborate: couldn't take over hosting: {error}"));
            if let Some(team) = self.team.as_mut() {
                // Let the next successor have its turn.
                team.lets_others_host = false;
            }
        }
    }

    fn try_take_over(&mut self, recover: bool) -> Result<(), String> {
        let team = self.team.as_mut().ok_or("not in a session")?;
        let data = team.data.clone().ok_or("no Collaborate folder")?;
        let root = team
            .files_folder()
            .map(Path::to_owned)
            .ok_or("this copy has no folder of the session's files")?;
        let address = team
            .card_address
            .clone()
            .ok_or("this computer has no address")?;
        let port: u16 = address
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .ok_or("this computer's address has no port")?;
        let identity = super::team::load_identity(&data).map_err(|e| e.to_string())?;
        let my_name = identity.name.clone();
        let cert = HostCert::load_or_make(&data.join("host-certificate.json"))
            .map_err(|e| format!("the host certificate: {e}"))?;
        let client: &mut Client = team.client_mut().ok_or("not joined")?;
        let list = client
            .members_list()
            .cloned()
            .ok_or("this copy wasn't sent the members list")?;
        let project_id = client.session().ok_or("no project ID")?.to_owned();
        let me = client.member();
        let next_id = client.next_id();
        let confirmed = client.confirmed(&mut self.world);
        // The files as the session had them (the folder is kept in step).
        let (store, _on_disk) = FolderStore::open(&root)
            .map_err(|e| format!("couldn't read the session's files: {e}"))?;
        let mut host = Host::new(session_world(confirmed.world))
            .with_graveyard(confirmed.graveyard)
            .with_files(confirmed.files, Box::new(store))
            .with_session(project_id.clone())
            .with_next_id(next_id);
        if recover && let Some(epoch) = confirmed.epoch {
            host = host.recover_from(epoch, confirmed.seq, team.recovery_wait);
        }
        if let Err(error) = host.start_journal(&root) {
            log::warn!("Collaborate: couldn't start the session's journal: {error}");
        }
        let access_path = members_file(&data, &root);
        // Where members will reach it, besides loopback (CW7.3).
        let binds = super::collab_trust::host_binds(
            port,
            &super::collab_trust::addresses_of(&address),
            self.team_panel.listen_everywhere,
            &mut self.output,
        );
        let mut session = HostSession::start_on(
            host,
            Access::from_list(&list),
            Some(access_path),
            &cert,
            &binds,
            Some(address.clone()),
        )
        .map_err(|e| format!("couldn't listen on port {port}: {e}"))?;
        session.set_trust(self.team_panel.trust);
        session.set_hosted_by(Some(me));
        let members = session.access().members_list();
        let loopback = session.loopback();
        let hosting = session.serve();
        team.link = Link::Net {
            remote: Box::new(Remote::join(
                loopback,
                cert.fingerprint(),
                None,
                Arc::new(identity),
            )),
            hosting: Some(hosting),
        };
        team.set_address(address.clone());
        team.lost_at = None;
        team.problem = None;
        if let Some(client) = team.client_mut() {
            client.disconnected();
        }
        if let Err(error) = super::collab_project::record_hosted(super::collab_project::Hosted {
            data: &data,
            id: &project_id,
            folder: &root,
            name: team.session_name.as_deref(),
            members,
            me,
            my_name: &my_name,
            address,
            cert: cert.fingerprint(),
        }) {
            log::warn!("Collaborate: couldn't keep this computer's record of the project: {error}");
        }
        self.output.info(if recover {
            "Collaborate: the host was lost, so you're hosting the session now (gathering \
             everyone's latest changes first)"
        } else {
            "Collaborate: the host left, so you're hosting the session now"
        });
        self.toasts
            .success("You're hosting the Collaborate session now");
        Ok(())
    }

    /// The hosting editor leaves with others still connected: the first
    /// successor takes the session on. Returns false when there's nobody
    /// to hand it to (the session ends as before).
    pub(super) fn hand_off_hosting(&mut self) -> bool {
        let has_successor = self
            .team
            .as_ref()
            .and_then(|t| t.hosting())
            .is_some_and(|h| !h.lock().host().successors().is_empty());
        if !has_successor {
            return false;
        }
        // The repository gets the session as it is now.
        self.repo_session_ending(axcenity_repo::checkpoint::Kind::Handoff);
        let Some(team) = self.team.as_mut() else {
            return false;
        };
        let Link::Net { hosting, .. } = &mut team.link else {
            return false;
        };
        let has_successor = hosting
            .as_ref()
            .is_some_and(|h| !h.lock().host().successors().is_empty());
        if !has_successor {
            return false;
        }
        let Some(hosting) = hosting.take() else {
            return false;
        };
        // Saved here: nothing of the session is left to recover in this
        // project (unsaved, its journal stays until a save).
        if self.history.is_saved() {
            hosting.lock().end_cleanly();
        }
        let to = hosting.hand_off();
        let name = to
            .and_then(|card| card.member)
            .and_then(|m| {
                team.roster()
                    .iter()
                    .find(|c| c.member == m)
                    .map(|c| c.profile.name.clone())
            })
            .unwrap_or_else(|| "another member".to_owned());
        self.output.info(format!(
            "Collaborate: you left; {name} is hosting the session now"
        ));
        true
    }
}

/// What a member is told when nobody on the successor list answered.
const ALONE: &str = "the host is gone and no one else could take over. Your copy stays open \
                     and your edits are kept; rejoin or host when you're ready.";
