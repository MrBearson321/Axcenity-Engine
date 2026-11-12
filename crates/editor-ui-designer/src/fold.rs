//! The inspector's sections, each under a header that folds, like
//! Properties' groups. The everyday ones (Styles, Position & anchor, Text, Widget,
//! Image, Look, Code & animation) start open; the fine-tuning ones
//! (States, Bevel, Slice) start folded, and their title says what the
//! element already has there ("Bevel · Raised"), so nothing set is hidden
//! without a word. A section stays open or folded as it was left, for
//! every element (kept in egui's memory by the section's name).

use axcenity_core::InstanceId;
use axcenity_ui::UiState;
use eframe::egui;

use super::DesignerScene;
use crate::theme::icons;

/// One section of the inspector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Styles,
    Anchor,
    Text,
    Widget,
    Image,
    Look,
    Code,
    States,
    Bevel,
    Slice,
}

impl Section {
    /// Its header, without what it has.
    pub fn title(self) -> &'static str {
        match self {
            Section::Styles => "Styles",
            Section::Anchor => "Position & anchor",
            Section::Text => "Text",
            Section::Widget => "Widget",
            Section::Image => "Image",
            Section::Look => "Look",
            Section::Code => "Code & animation",
            Section::States => "States",
            Section::Bevel => "Bevel",
            Section::Slice => "Slice",
        }
    }

    /// What's in it, on hover over the header.
    pub fn hint(self) -> &'static str {
        match self {
            Section::Styles => "Ready-made looks: one click styles it, hover and pressed too",
            Section::Anchor => {
                "Where it is and how big, in design pixels, and what it stays pinned to \
                 when the screen is another size: an edge, the middle, or stretched across"
            }
            Section::Text => "Bold, alignment, size, colour, outline and shadow of its text",
            Section::Widget => "Its value, range and the way it shows",
            Section::Image => "The picture it shows",
            Section::Look => "How it's turned, and how see-through each part is",
            Section::Code => {
                "Its path for scripts, code that runs when it's clicked or changed, the \
                 scripts that use it, and its animations"
            }
            Section::States => {
                "How it looks while hovered, pressed or disabled, and how fast it changes"
            }
            Section::Bevel => "Edges lit and shaded so it looks raised or pressed in",
            Section::Slice => {
                "A picture as a frame whose corners keep their size while the rest stretches"
            }
        }
    }

    /// Whether it starts open: the fine-tuning ones start folded.
    pub fn starts_open(self) -> bool {
        !matches!(self, Section::States | Section::Bevel | Section::Slice)
    }

    fn icon(self) -> &'static str {
        match self {
            Section::Styles => icons::PALETTE,
            Section::Anchor => icons::ANCHOR,
            Section::Text => icons::TEXT_AA,
            Section::Widget => icons::SLIDERS_HORIZONTAL,
            Section::Image => icons::IMAGE,
            Section::Look => icons::EYE,
            Section::Code => icons::CODE,
            Section::States => icons::CURSOR_CLICK,
            Section::Bevel => icons::SQUARE_HALF,
            Section::Slice => icons::FRAME_CORNERS,
        }
    }

    /// The id its open state is kept under in egui's memory: one per
    /// section, so it stays as it was left from one element to the next.
    fn id(self) -> egui::Id {
        egui::Id::new(("ui designer section", self.title()))
    }

    /// Whether it's open: as it was left, else as it starts.
    pub fn is_open(self, ctx: &egui::Context) -> bool {
        ctx.data(|data| data.get_temp::<bool>(self.id()))
            .unwrap_or(self.starts_open())
    }

    /// Opens or folds it, as a click on its header would.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_open(self, ctx: &egui::Context, open: bool) {
        ctx.data_mut(|data| data.insert_temp(self.id(), open));
    }
}

/// Draws `section` under its header, `title` (its name, or its name and
/// what it has), and `contents` in it while it's open. Returns whether it
/// is open.
pub fn fold(
    ui: &mut egui::Ui,
    section: Section,
    title: &str,
    contents: impl FnOnce(&mut egui::Ui),
) -> bool {
    crate::widgets::section(section.icon(), title)
        .id(section.id())
        .default_open(section.starts_open())
        .remember()
        .hint(section.hint())
        .show(ui, contents)
}

/// `section`'s header: its name, then what the element has there when it
/// starts folded, so a folded section never hides something set.
pub fn title(scene: &DesignerScene, id: InstanceId, section: Section) -> String {
    let has = match section {
        Section::States => {
            let states = super::states::styled_states(scene.world, id, scene.stylesheets);
            (!states.is_empty()).then(|| {
                states
                    .iter()
                    .map(|state: &UiState| state.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
        }
        Section::Bevel => super::bevel::current(scene, id)
            .map(|bevel| if bevel.raised { "Raised" } else { "Sunken" }.to_owned()),
        Section::Slice => super::slice::has_slice(scene, id).then(|| "On".to_owned()),
        _ => None,
    };
    match has {
        Some(has) => format!("{} · {has}", section.title()),
        None => section.title().to_owned(),
    }
}
