//! Listening to a sound result before getting it (roadmap G1): the listen
//! button on a Sounds card and in its drawer fetches the source's preview
//! (`AssetRecord::preview_url`) on a thread, and hands its bytes to the
//! editor ([`ToolboxAction::PlaySound`]), which plays them on their own.
//! Nothing is written anywhere: the preview isn't the package Get fetches.

use super::*;

/// The most a sound preview may be. Openverse's previews are the source's
/// own short files; a bigger one is refused rather than held in memory.
pub(super) const LISTEN_LIMIT: u64 = 16 * 1024 * 1024;

/// The sound being listened to.
pub(super) enum Listen {
    /// Its preview is on its way.
    Fetching {
        key: String,
        name: String,
        rx: Receiver<Result<Vec<u8>, String>>,
    },
    /// The editor is playing it.
    Playing { key: String },
}

impl Listen {
    fn key(&self) -> &str {
        match self {
            Listen::Fetching { key, .. } | Listen::Playing { key } => key,
        }
    }
}

/// The preview at `url`, checked to be https and at most [`LISTEN_LIMIT`].
pub(super) fn fetch_preview(http: &dyn Http, url: &str) -> Result<Vec<u8>, String> {
    if !url.starts_with("https://") {
        return Err("its preview isn't an https address".to_owned());
    }
    http::get_bytes(http, url, LISTEN_LIMIT).map_err(|err| err.to_string())
}

impl ToolboxPanel {
    /// Whether the result `key` is being fetched or played.
    pub(super) fn listening_to(&self, key: &str) -> bool {
        self.listen
            .as_ref()
            .is_some_and(|listen| listen.key() == key)
    }

    /// The listen button was pressed for the result `key`: stop it if it's
    /// the one heard, else fetch its preview at `url` and play it (stopping
    /// any other).
    pub(super) fn toggle_listen(&mut self, ctx: &egui::Context, key: &str, name: &str, url: &str) {
        if self.listening_to(key) {
            self.stop_listening();
            return;
        }
        self.stop_listening();
        if url.is_empty() {
            self.error = Some(format!("{name} has no preview to listen to."));
            return;
        }
        let (tx, rx) = mpsc::channel();
        let url = url.to_owned();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let client = http::Client::new(Duration::from_secs(30));
            let _ = tx.send(fetch_preview(&client, &url));
            ctx.request_repaint();
        });
        self.listen = Some(Listen::Fetching {
            key: key.to_owned(),
            name: name.to_owned(),
            rx,
        });
    }

    /// Stops what's being listened to, telling the editor if it plays.
    pub(super) fn stop_listening(&mut self) {
        if let Some(Listen::Playing { .. }) = self.listen.take() {
            self.sound_out.push(ToolboxAction::StopSound);
        }
    }

    /// The preview, once fetched, goes to the editor to play.
    pub(super) fn poll_listen(&mut self) {
        let Some(Listen::Fetching { key, name, rx }) = self.listen.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(bytes)) => {
                self.sound_out
                    .push(ToolboxAction::PlaySound { name, bytes });
                self.listen = Some(Listen::Playing { key });
            }
            Ok(Err(err)) => {
                self.error = Some(format!("Could not listen to {name}: {err}"));
            }
            Err(TryRecvError::Empty) => self.listen = Some(Listen::Fetching { key, name, rx }),
            Err(TryRecvError::Disconnected) => {
                self.error = Some(format!("Could not listen to {name}."));
            }
        }
    }

    /// The editor's player has stopped (the preview ended, Play started or
    /// there's no audio output): the button shows listen again.
    pub fn sound_ended(&mut self) {
        if let Some(Listen::Playing { .. }) = self.listen {
            self.listen = None;
        }
    }

    /// The listen button was clicked for the result `key`: cancel its
    /// preview while it comes, else play or stop it.
    pub(super) fn listen_clicked(&mut self, ctx: &egui::Context, key: &str, name: &str, url: &str) {
        if matches!(&self.listen, Some(Listen::Fetching { key: fetching, .. }) if fetching == key) {
            // Cancelled: the thread's answer is dropped with the channel.
            self.listen = None;
        } else {
            self.toggle_listen(ctx, key, name, url);
        }
    }
}

/// The listen button for the result `key` (play, an hourglass while its
/// preview comes, stop while it plays). True when clicked.
pub(super) fn button(
    ui: &mut egui::Ui,
    listen: &Option<Listen>,
    key: &str,
    name: &str,
    url: &str,
) -> bool {
    let (icon, tip) = match listen {
        Some(Listen::Playing { key: playing }) if playing == key => (icons::STOP, "Stop"),
        Some(Listen::Fetching { key: fetching, .. }) if fetching == key => {
            (icons::HOURGLASS, "Getting the preview… (click to cancel)")
        }
        _ => (icons::PLAY, "Listen"),
    };
    ui.add_enabled(!url.is_empty(), egui::Button::new(icon))
        .on_hover_text(tip)
        .on_disabled_hover_text(format!("{name} has no preview to listen to."))
        .clicked()
}

#[cfg(test)]
mod tests {
    use axcenity_toolbox::http::Fixture;

    use super::*;

    const URL: &str = "https://cdn.freesound.org/previews/1/birds.mp3";

    #[test]
    fn a_preview_is_fetched_only_over_https() {
        let http = Fixture::default().with(URL, b"ID3 birds".to_vec());
        assert_eq!(fetch_preview(&http, URL).unwrap(), b"ID3 birds");
        let err = fetch_preview(&http, "http://example.com/birds.mp3").unwrap_err();
        assert!(err.contains("https"), "{err}");
        assert!(fetch_preview(&http, "https://cdn.freesound.org/none.mp3").is_err());
    }

    /// The preview goes to the editor once fetched; pressing again or
    /// another result stops it; the editor's player stopping resets it.
    #[test]
    fn listening_plays_the_preview_then_stops_it() {
        let ctx = egui::Context::default();
        let mut panel = ToolboxPanel::idle();
        let (tx, rx) = mpsc::channel();
        panel.listen = Some(Listen::Fetching {
            key: "openverse:birds".into(),
            name: "Birds".into(),
            rx,
        });
        panel.poll_listen();
        assert!(panel.sound_out.is_empty(), "still coming");
        assert!(panel.listening_to("openverse:birds"));
        tx.send(Ok(b"ID3 birds".to_vec())).unwrap();
        panel.poll_listen();
        assert_eq!(
            panel.sound_out.drain(..).collect::<Vec<_>>(),
            [ToolboxAction::PlaySound {
                name: "Birds".into(),
                bytes: b"ID3 birds".to_vec(),
            }]
        );
        assert!(matches!(panel.listen, Some(Listen::Playing { .. })));

        // Pressed again: stopped.
        panel.listen_clicked(&ctx, "openverse:birds", "Birds", URL);
        assert_eq!(
            panel.sound_out.drain(..).collect::<Vec<_>>(),
            [ToolboxAction::StopSound]
        );
        assert!(panel.listen.is_none());

        // The editor's player stopped by itself: listen shows again.
        panel.listen = Some(Listen::Playing {
            key: "openverse:birds".into(),
        });
        panel.sound_ended();
        assert!(panel.listen.is_none());
        assert!(panel.sound_out.is_empty());

        // A preview that can't be fetched says why, and nothing plays.
        let (tx, rx) = mpsc::channel();
        panel.listen = Some(Listen::Fetching {
            key: "openverse:rain".into(),
            name: "Rain".into(),
            rx,
        });
        tx.send(Err("offline".into())).unwrap();
        panel.poll_listen();
        assert!(panel.listen.is_none());
        assert!(panel.sound_out.is_empty());
        assert_eq!(
            panel.error.as_deref(),
            Some("Could not listen to Rain: offline")
        );

        // No preview at all.
        panel.listen_clicked(&ctx, "openverse:quiet", "Quiet", "");
        assert!(panel.listen.is_none());
        assert!(panel.error.as_deref().unwrap().contains("no preview"));

        // Leaving the Sounds tab stops what plays.
        panel.set_kind(Kind::Sound);
        panel.listen = Some(Listen::Playing {
            key: "openverse:birds".into(),
        });
        panel.set_kind(Kind::Texture);
        assert_eq!(
            panel.sound_out.drain(..).collect::<Vec<_>>(),
            [ToolboxAction::StopSound]
        );
        assert!(panel.listen.is_none());
    }
}
