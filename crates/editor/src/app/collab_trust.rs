//! Who gets into a session this editor hosts, and where it listens
//! (cohesion pass CW7.2–7.3): Settings > Collaborate's "Who gets in" and
//! "Listening", and the window that asks the person hosting about each
//! joiner the setting says to ask about.

use std::net::{IpAddr, SocketAddr, TcpListener, ToSocketAddrs};

use axcenity_collab::handshake::Trust;
use axcenity_collab::session::listen_addresses;

use super::*;

/// What each trust choice is called and does, for Settings.
pub(crate) fn trust_words(trust: Trust) -> (&'static str, &'static str) {
    match trust {
        Trust::InviteHolders => (
            "Anyone with an invite",
            "A valid invite lets someone in, and they're a member from then on. Members \
             rejoin without asking.",
        ),
        Trust::AskNewMembers => (
            "Ask before new members join",
            "Members rejoin without asking. Someone using an invite for the first time \
             waits until you let them in.",
        ),
        Trust::AskEveryone => (
            "Ask every time",
            "Everyone waits until you let them in, members too.",
        ),
    }
}

/// Where a host on `port` listens: loopback and `reach` (or this
/// computer's network address), or every interface when the setting says
/// so or one of those can't be listened on (said in Output).
pub(crate) fn host_binds(
    port: u16,
    reach: &[IpAddr],
    everywhere: bool,
    output: &mut crate::output::Output,
) -> Vec<SocketAddr> {
    let binds = listen_addresses(port, reach, everywhere);
    // Port 0 picks a free port each time, so only a fixed one is tried.
    let free = port == 0
        || binds
            .iter()
            .all(|address| TcpListener::bind(address).is_ok());
    if free {
        return binds;
    }
    output.warning(format!(
        "Collaborate: couldn't listen on {} only, so listening on every network",
        binds
            .iter()
            .map(|a| a.ip().to_string())
            .collect::<Vec<_>>()
            .join(" and ")
    ));
    listen_addresses(port, &[], true)
}

/// The addresses `address` ("name:port") names, for a host that will be
/// reached by it.
pub(crate) fn addresses_of(address: &str) -> Vec<IpAddr> {
    address
        .to_socket_addrs()
        .map(|found| found.map(|a| a.ip()).collect())
        .unwrap_or_default()
}

impl EditorApp {
    /// Settings > Collaborate's "Who gets in" and "Listening".
    pub(super) fn trust_settings(&mut self, ui: &mut egui::Ui) {
        use super::settings;
        let palette = theme::palette(ui.ctx());
        settings::section(ui, "Who gets in");
        let mut trust = self.team_panel.trust;
        for choice in Trust::ALL {
            let (label, help) = trust_words(choice);
            ui.radio_value(&mut trust, choice, label)
                .on_hover_text(help);
        }
        ui.add(
            egui::Label::new(egui::RichText::new(trust_words(trust).1).color(palette.text_weak))
                .wrap(),
        );
        if trust != self.team_panel.trust {
            self.team_panel.trust = trust;
            if let Some(hosting) = self.team.as_ref().and_then(|t| t.hosting()) {
                hosting.lock().set_trust(trust);
            }
        }
        ui.add(
            egui::Label::new(
                egui::RichText::new(
                    "Remembered members are listed in the Collaborate panel: Remove takes \
                     someone out, and they need a new invite to come back.",
                )
                .color(palette.text_weak),
            )
            .wrap(),
        );
        ui.add_space(theme::space::M);
        settings::section(ui, "Listening");
        ui.checkbox(
            &mut self.team_panel.listen_everywhere,
            "Listen on every network",
        );
        ui.add(
            egui::Label::new(
                egui::RichText::new(
                    "Off (recommended): a session you host takes connections only over \
                     Tailscale, or only on the network others reach you by, and from this \
                     computer. On: from every network this computer is on. Takes effect the \
                     next time you host.",
                )
                .color(palette.text_weak),
            )
            .wrap(),
        );
    }

    /// The window asking the person hosting about each joiner waiting to
    /// be let in.
    pub(super) fn join_requests_window(&mut self, ctx: &egui::Context) {
        let Some(hosting) = self.team.as_ref().and_then(|t| t.hosting()) else {
            return;
        };
        let requests = hosting.lock().join_requests().to_vec();
        // Asked joiners arrive on the network thread: look again soon.
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
        if requests.is_empty() {
            return;
        }
        let mut answers = Vec::new();
        let mut open = true;
        widgets::tool_window(icons::USER_PLUS, "Someone wants to join")
            .id("collaborate join requests")
            .show(ctx, &mut open, |ui| {
                let palette = theme::palette(ui.ctx());
                ui.set_max_width(360.0);
                for request in &requests {
                    let asking = &request.asking;
                    ui.label(egui::RichText::new(&asking.name).strong());
                    let who = if asking.member {
                        format!("A member, as {:?}", asking.role)
                    } else {
                        format!("New, with an invite as {:?}", asking.role)
                    };
                    let over = if axcenity_collab::tailscale::is_tailscale_ip(request.address) {
                        " (Tailscale)"
                    } else {
                        ""
                    };
                    for line in [
                        who,
                        format!("Device key {}", asking.key.fingerprint().short()),
                        format!("From {}{over}", request.address),
                    ] {
                        ui.label(egui::RichText::new(line).color(palette.text_weak));
                    }
                    ui.horizontal(|ui| {
                        let let_in = widgets::button(
                            widgets::Role::Primary,
                            Some(icons::CHECK),
                            "Let in",
                            &palette,
                        );
                        if ui.add(let_in).clicked() {
                            answers.push((request.id, true));
                        }
                        let away =
                            widgets::button(widgets::Role::Secondary, None, "Turn away", &palette);
                        if ui.add(away).clicked() {
                            answers.push((request.id, false));
                        }
                    });
                    ui.add_space(theme::space::S);
                }
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "Only let in people you expect. Anyone not answered in two minutes \
                             is turned away. Settings > Collaborate chooses who's asked about.",
                        )
                        .color(palette.text_weak),
                    )
                    .wrap(),
                );
            });
        if !open {
            // Closing the window turns everyone waiting away.
            answers.extend(requests.iter().map(|r| (r.id, false)));
        }
        let mut session = hosting.lock();
        for (id, allow) in answers {
            session.answer_join(id, allow);
        }
    }
}
