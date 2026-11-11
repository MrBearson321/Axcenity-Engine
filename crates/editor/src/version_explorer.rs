//! The Version Explorer tab (Help > Version Explorer): this build's version,
//! the release history, features, known issues and a comparison of two
//! versions. It reads the release files built into the editor (`build.rs`),
//! so it works offline. Design: `docs/research/release-system/architecture.md`
//! §6; the data and its queries: `axcenity_release::History`.

use axcenity_release::{
    BuildInfo, Category, Evidence, Feature, FeatureStatus, History, IssueState, IssueStatus,
    ReleaseRecord, Subsystem, Version, VersionRef,
};
use eframe::egui::{self, RichText};

use crate::theme::{self, icons};
use crate::widgets;

include!(concat!(env!("OUT_DIR"), "/release_files.rs"));

/// The public history built into this editor: no internal entries.
pub fn built_in_history() -> History {
    History::from_sources(RELEASE_FILES.iter().copied())
        .expect("the release files were checked by `axcenity-release check`")
        .public()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Page {
    #[default]
    Overview,
    Releases,
    Features,
    Issues,
    Compare,
}

impl Page {
    const ALL: [Page; 5] = [
        Page::Overview,
        Page::Releases,
        Page::Features,
        Page::Issues,
        Page::Compare,
    ];

    fn label(self) -> &'static str {
        match self {
            Page::Overview => "Overview",
            Page::Releases => "Releases",
            Page::Features => "Features",
            Page::Issues => "Issues",
            Page::Compare => "Compare",
        }
    }
}

/// The tab's state. The history loads the first time the tab is drawn.
#[derive(Default)]
pub struct VersionExplorer {
    history: Option<History>,
    pub page: Page,
    search: String,
    subsystem: Option<Subsystem>,
    /// Compare's two ends: a version, or "unreleased" for `to`.
    from: String,
    to: String,
}

impl VersionExplorer {
    pub fn show(&mut self, ui: &mut egui::Ui, build: &BuildInfo) {
        if self.history.is_none() {
            self.history = Some(built_in_history());
        }
        ui.horizontal_wrapped(|ui| {
            for page in Page::ALL {
                if widgets::chip(ui, page.label(), self.page == page).clicked() {
                    self.page = page;
                }
            }
        });
        ui.add_space(theme::space::XS);
        widgets::divider(ui);
        let history = self.history.take().expect("loaded above");
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .id_salt(("version-explorer", self.page))
            .show(ui, |ui| match self.page {
                Page::Overview => overview(ui, &history, build),
                Page::Releases => releases(ui, &history),
                Page::Features => self.features(ui, &history),
                Page::Issues => self.issues(ui, &history, &build.version),
                Page::Compare => self.compare(ui, &history, &build.version),
            });
        self.history = Some(history);
    }

    fn filters(&mut self, ui: &mut egui::Ui, hint: &str) {
        let width = (ui.available_width() - widgets::SEARCH_FIELD_MARGINS).max(80.0);
        widgets::search_field(ui, &mut self.search, hint, width);
        let label = self.subsystem.map_or("Every area", Subsystem::name);
        egui::ComboBox::from_id_salt("version-explorer-subsystem")
            .selected_text(label)
            .width(ui.available_width().min(220.0))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.subsystem, None, "Every area");
                for subsystem in Subsystem::ALL {
                    ui.selectable_value(&mut self.subsystem, Some(subsystem), subsystem.name());
                }
            });
        ui.add_space(theme::space::XS);
    }

    fn matches(&self, subsystem: Subsystem, texts: &[&str]) -> bool {
        let words = self.search.to_lowercase();
        self.subsystem.is_none_or(|s| s == subsystem)
            && words
                .split_whitespace()
                .all(|word| texts.iter().any(|text| text.to_lowercase().contains(word)))
    }

    fn features(&mut self, ui: &mut egui::Ui, history: &History) {
        self.filters(ui, "Search features");
        let shown: Vec<&Feature> = history
            .features()
            .iter()
            .filter(|f| self.matches(f.subsystem, &[&f.name, &f.id, &f.description]))
            .collect();
        if shown.is_empty() {
            widgets::empty_state(
                ui,
                icons::MAGNIFYING_GLASS,
                "No features match",
                "Try fewer words or every area.",
                None,
            );
            return;
        }
        for feature in shown {
            let title = format!("{} · {}", feature.name, feature.introduced.version);
            let short = header_text(ui, &title);
            widgets::section(icons::PUZZLE_PIECE, &short)
                .hint(&title)
                .id_salt(&feature.id)
                .default_open(false)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        badge(
                            ui,
                            status_label(feature.status),
                            status_color(ui, feature.status),
                        );
                        badge(
                            ui,
                            evidence_label(feature.introduced.evidence),
                            evidence_color(ui, feature.introduced.evidence),
                        );
                        ui.label(weak(ui, feature.subsystem.name()));
                    });
                    ui.add(egui::Label::new(&feature.description).wrap());
                    if !feature.introduced.source.is_empty() {
                        ui.label(weak(ui, &format!("Source: {}", feature.introduced.source)));
                    }
                    if let Ok(timeline) = history.feature_timeline(&feature.id) {
                        ui.add_space(theme::space::XS);
                        widgets::section_label(ui, "Timeline");
                        for row in timeline {
                            let mut text = format!("{}  {}", row.version, row.change);
                            if row.evidence != Evidence::Verified {
                                text.push_str(&format!(
                                    " ({})",
                                    evidence_label(row.evidence).to_lowercase()
                                ));
                            }
                            ui.add(egui::Label::new(text).wrap());
                        }
                    }
                });
        }
    }

    fn issues(&mut self, ui: &mut egui::Ui, history: &History, installed: &Version) {
        self.filters(ui, "Search known issues");
        let shown: Vec<_> = history
            .issues()
            .iter()
            .filter(|i| self.matches(i.subsystem, &[&i.id, &i.title, &i.workaround, &i.details]))
            .collect();
        if shown.is_empty() {
            widgets::empty_state(
                ui,
                icons::BUG,
                "No known issues match",
                "Try fewer words or every area.",
                None,
            );
            return;
        }
        for issue in shown {
            let title = format!("{}  {}", issue.id, issue.title);
            let short = header_text(ui, &title);
            widgets::section(icons::BUG, &short)
                .hint(&title)
                .id_salt(&issue.id)
                .default_open(false)
                .show(ui, |ui| {
                    let state = history
                        .issue_status(&issue.id, installed)
                        .unwrap_or(IssueState::NotInvestigated);
                    ui.horizontal_wrapped(|ui| {
                        let (text, color) = match issue.status {
                            IssueStatus::Open => ("Open", theme::palette(ui.ctx()).warning),
                            IssueStatus::Fixed => ("Fixed", theme::palette(ui.ctx()).success),
                            IssueStatus::WontFix => {
                                ("Won't fix", theme::palette(ui.ctx()).text_weak)
                            }
                        };
                        badge(ui, text, color);
                        ui.label(weak(ui, issue.subsystem.name()));
                    });
                    ui.add(
                        egui::Label::new(format!(
                            "This version ({installed}): {}",
                            issue_state_text(&state)
                        ))
                        .wrap(),
                    );
                    if let Some(fixed) = history.fixed_in(issue) {
                        ui.label(format!("Fixed in {fixed}"));
                    }
                    if !issue.workaround.is_empty() {
                        ui.add(
                            egui::Label::new(format!("Workaround: {}", issue.workaround)).wrap(),
                        );
                    }
                    if !issue.details.is_empty() {
                        ui.add(egui::Label::new(weak(ui, &issue.details)).wrap());
                    }
                });
        }
    }

    fn compare(&mut self, ui: &mut egui::Ui, history: &History, installed: &Version) {
        let versions: Vec<String> = history
            .releases()
            .iter()
            .rev()
            .map(|r| r.version.to_string())
            .collect();
        if versions.is_empty() {
            widgets::empty_state(
                ui,
                icons::GIT_DIFF,
                "No releases yet",
                "Compare needs at least one release.",
                None,
            );
            return;
        }
        if self.from.is_empty() {
            self.from = history
                .releases()
                .iter()
                .rev()
                .find(|r| r.version <= *installed)
                .map_or_else(|| versions[0].clone(), |r| r.version.to_string());
        }
        if self.to.is_empty() {
            self.to = "unreleased".to_owned();
        }
        crate::responsive::form(ui, "version-explorer-compare", |ui| {
            ui.label("After");
            version_picker(ui, "compare-from", &mut self.from, &versions, false);
            ui.end_row();
            ui.label("Up to");
            version_picker(ui, "compare-to", &mut self.to, &versions, true);
            ui.end_row();
        });
        ui.add_space(theme::space::XS);
        let result = history
            .version(&self.from, false)
            .and_then(|from| Ok((from, history.version(&self.to, true)?)))
            .and_then(|(from, to)| history.compare(&from, &to));
        let comparison = match result {
            Ok(comparison) => comparison,
            Err(err) => {
                ui.colored_label(theme::palette(ui.ctx()).warning, err.to_string());
                return;
            }
        };
        if !comparison.formats.is_empty() {
            widgets::section_label(ui, "File formats");
            for format in &comparison.formats {
                let show = |n: Option<u32>| n.map_or("none".to_owned(), |n| n.to_string());
                ui.label(format!(
                    "{}: {} → {}",
                    format.format,
                    show(format.from),
                    show(format.to)
                ));
            }
            ui.add_space(theme::space::XS);
        }
        if comparison.changes.is_empty() {
            ui.label(weak(ui, "No recorded changes in this range."));
        }
        for (category, changes) in &comparison.changes {
            if *category == Category::Internal {
                continue;
            }
            widgets::section_label(ui, category.heading());
            for change in changes {
                change_row(
                    ui,
                    &change.title,
                    change.subsystem,
                    &change.version,
                    change.experimental,
                    change.evidence,
                );
            }
            ui.add_space(theme::space::XS);
        }
    }
}

fn overview(ui: &mut egui::Ui, history: &History, build: &BuildInfo) {
    let palette = theme::palette(ui.ctx());
    ui.label(
        RichText::new(format!("Axcenity {}", build.version))
            .size(theme::type_scale::HEADING)
            .family(theme::medium_family())
            .color(palette.text_strong),
    );
    ui.label(weak(ui, &build.summary()));
    ui.add_space(theme::space::S);
    match history.release(&build.version) {
        Some(record) => {
            widgets::section_label(ui, "This release");
            release_body(ui, record);
        }
        None => {
            if let Some(latest) = history.latest_published() {
                ui.add(
                    egui::Label::new(format!(
                        "{} isn't released yet. The latest release is {}.",
                        build.version, latest.version
                    ))
                    .wrap(),
                );
            }
            let pending = history.unreleased().len();
            if pending > 0 {
                ui.label(weak(
                    ui,
                    &format!("{pending} recorded changes since then: see Compare."),
                ));
            }
        }
    }
    ui.add_space(theme::space::S);
    widgets::section_label(ui, "Known issues in this version");
    let affecting = history.issues_affecting(&build.version);
    if affecting.is_empty() {
        ui.label(weak(
            ui,
            "None known. Issues nobody has checked against this version aren't listed.",
        ));
    }
    for (issue, state) in affecting {
        ui.add(
            egui::Label::new(format!(
                "{}  {} ({})",
                issue.id,
                issue.title,
                issue_state_text(&state)
            ))
            .wrap(),
        );
        if !issue.workaround.is_empty() {
            ui.add(egui::Label::new(weak(ui, &format!("Workaround: {}", issue.workaround))).wrap());
        }
    }
}

fn releases(ui: &mut egui::Ui, history: &History) {
    for record in history.releases().iter().rev() {
        let title = format!("{} · {}", record.version, record.title);
        let short = header_text(ui, &title);
        let salt = record.version.to_string();
        widgets::section(icons::TAG, &short)
            .hint(&title)
            .id_salt(&salt)
            .default_open(false)
            .show(ui, |ui| release_body(ui, record));
    }
}

/// `title` cut with "…" to fit one section header in `ui`'s width (headers
/// don't wrap); the whole title shows on hover.
fn header_text(ui: &egui::Ui, title: &str) -> String {
    // The chevron, the icon and the gaps beside them.
    let room = ui.available_width() - 44.0;
    let font = egui::FontId::new(theme::type_scale::BODY, theme::medium_family());
    let width = |text: &str| {
        ui.painter()
            .layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
            .size()
            .x
    };
    if width(title) <= room {
        return title.to_owned();
    }
    let mut cut: Vec<char> = title.chars().collect();
    while !cut.is_empty() {
        cut.pop();
        let text = format!("{}…", cut.iter().collect::<String>().trim_end());
        if width(&text) <= room {
            return text;
        }
    }
    "…".to_owned()
}

fn release_body(ui: &mut egui::Ui, record: &ReleaseRecord) {
    ui.horizontal_wrapped(|ui| {
        ui.label(weak(ui, &record.date));
        badge(
            ui,
            record.version.channel().name(),
            theme::palette(ui.ctx()).accent_text,
        );
        if !record.published {
            badge(ui, "Never published", theme::palette(ui.ctx()).text_weak);
        }
        badge(
            ui,
            evidence_label(record.evidence),
            evidence_color(ui, record.evidence),
        );
    });
    if !record.summary.is_empty() {
        ui.add(egui::Label::new(record.summary.trim()).wrap());
    }
    if !record.note.is_empty() {
        ui.add(egui::Label::new(weak(ui, record.note.trim())).wrap());
    }
    for category in Category::ALL {
        let changes: Vec<_> = record
            .changes
            .iter()
            .filter(|c| c.category == category && c.is_public())
            .collect();
        if changes.is_empty() {
            continue;
        }
        ui.add_space(theme::space::XS);
        widgets::section_label(ui, category.heading());
        for change in changes {
            let experimental = change.stability == axcenity_release::Stability::Experimental;
            change_row(
                ui,
                &change.title,
                change.subsystem,
                &VersionRef::Release(record.version.clone()),
                experimental,
                change.evidence,
            );
        }
    }
    for correction in &record.corrections {
        ui.add(
            egui::Label::new(weak(
                ui,
                &format!("Correction to {}: {}", correction.release, correction.text),
            ))
            .wrap(),
        );
    }
}

fn change_row(
    ui: &mut egui::Ui,
    title: &str,
    subsystem: Subsystem,
    version: &VersionRef,
    experimental: bool,
    evidence: Evidence,
) {
    // The title, then its area quietly after it: titles often start with
    // the area's name already ("Multiplayer: host and join…").
    let palette = theme::palette(ui.ctx());
    let mut job = egui::text::LayoutJob::default();
    let style = ui.style();
    let mut add = |text: &str, color: egui::Color32| {
        RichText::new(text).color(color).append_to(
            &mut job,
            style,
            egui::FontSelection::Default,
            egui::Align::Center,
        );
    };
    add(title, palette.text);
    if experimental {
        add(" (experimental)", palette.warning);
    }
    add(&format!("  {}", subsystem.name()), palette.text_weak);
    let response = ui.add(egui::Label::new(job).wrap());
    let mut tip = format!("In {version}");
    if evidence != Evidence::Verified {
        tip.push_str(&format!(" · {}", evidence_label(evidence)));
    }
    response.on_hover_text(tip);
}

fn version_picker(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut String,
    versions: &[String],
    unreleased: bool,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.as_str())
        .width(ui.available_width().min(200.0))
        .show_ui(ui, |ui| {
            if unreleased {
                ui.selectable_value(value, "unreleased".to_owned(), "unreleased");
            }
            for version in versions {
                ui.selectable_value(value, version.clone(), version.as_str());
            }
        });
}

fn badge(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(widgets::micro(text, color));
}

fn weak(ui: &egui::Ui, text: &str) -> RichText {
    RichText::new(text).color(theme::palette(ui.ctx()).text_weak)
}

fn evidence_label(evidence: Evidence) -> &'static str {
    match evidence {
        Evidence::Verified => "Verified",
        Evidence::Reconstructed => "Reconstructed",
        Evidence::Unknown => "Unknown",
    }
}

fn evidence_color(ui: &egui::Ui, evidence: Evidence) -> egui::Color32 {
    let palette = theme::palette(ui.ctx());
    match evidence {
        Evidence::Verified => palette.success,
        Evidence::Reconstructed => palette.warning,
        Evidence::Unknown => palette.text_weak,
    }
}

fn status_label(status: FeatureStatus) -> &'static str {
    match status {
        FeatureStatus::Active => "Active",
        FeatureStatus::Experimental => "Experimental",
        FeatureStatus::Deprecated => "Deprecated",
        FeatureStatus::Removed => "Removed",
    }
}

fn status_color(ui: &egui::Ui, status: FeatureStatus) -> egui::Color32 {
    let palette = theme::palette(ui.ctx());
    match status {
        FeatureStatus::Active => palette.success,
        FeatureStatus::Experimental => palette.accent_text,
        FeatureStatus::Deprecated => palette.warning,
        FeatureStatus::Removed => palette.text_weak,
    }
}

/// The four-state answer in words. "Not investigated" never means "not
/// affected".
pub fn issue_state_text(state: &IssueState) -> String {
    match state {
        IssueState::Fixed {
            fixed_in: VersionRef::Unreleased,
        } => "fixed in this build; not released yet".to_owned(),
        IssueState::Fixed { fixed_in } => format!("fixed in {fixed_in}"),
        IssueState::ConfirmedAffected => "affected (confirmed)".to_owned(),
        IssueState::AffectedByRange => {
            "affected (by its version range; not checked on this version)".to_owned()
        }
        IssueState::BeforeIntroduced { introduced } => {
            format!("not affected: it arrived in {introduced}")
        }
        IssueState::NotInvestigated => "not investigated".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_history_is_the_repositorys_public_history() {
        let history = built_in_history();
        assert!(history.validate().is_empty(), "{:?}", history.validate());
        let alpha3 = Version::parse("0.1.0-alpha.3").unwrap();
        assert!(history.release(&alpha3).is_some_and(|r| r.published));
        assert!(!history.features().is_empty());
        assert!(history.changes().all(|(_, c)| c.is_public()));
        assert_eq!(
            history.formats().get("project"),
            Some(&axcenity_project::FORMAT_VERSION)
        );
    }

    #[test]
    fn every_page_draws() {
        let ctx = theme::test_context();
        let build = BuildInfo::current();
        let mut explorer = VersionExplorer::default();
        for page in Page::ALL {
            explorer.page = page;
            // Open every collapsible section, so their bodies draw too.
            explorer.search.clear();
            for _ in 0..2 {
                let _ = crate::theme::run(&ctx, egui::RawInput::default(), |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.set_width(260.0);
                        explorer.show(ui, &build);
                    });
                });
            }
        }
        assert_eq!(explorer.to, "unreleased");
        assert!(!explorer.from.is_empty());
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            for page in Page::ALL {
                explorer.page = page;
                let path = std::path::Path::new(&folder).join(format!(
                    "version-explorer-{}.png",
                    page.label().to_lowercase()
                ));
                crate::snapshot::snapshot(
                    &path,
                    egui::vec2(300.0, 720.0),
                    vec![Vec::new(); 3],
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| explorer.show(ui, &build));
                    },
                );
            }
        }
    }

    #[test]
    fn the_search_needs_every_word_and_the_area() {
        let mut explorer = VersionExplorer {
            search: "terrain SCULPT".to_owned(),
            ..VersionExplorer::default()
        };
        assert!(explorer.matches(Subsystem::Terrain, &["Terrain Studio", "Sculpt and paint"]));
        assert!(!explorer.matches(Subsystem::Terrain, &["Terrain Studio"]));
        explorer.subsystem = Some(Subsystem::Physics);
        assert!(!explorer.matches(Subsystem::Terrain, &["Terrain Studio", "Sculpt"]));
    }
}
