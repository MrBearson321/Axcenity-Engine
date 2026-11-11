//! How the start screen's pages are reached: a sidebar down the left (wide,
//! or folded to its icons) or a bar across the top. Both list the same
//! pages and Settings; which one shows, and whether the sidebar is folded,
//! is saved between runs ([`StartNav::to_text`]).
//!
//! Folding slides the rail. Labels only appear once the rail is wide enough
//! to hold them, so nothing is clipped mid-slide; the open and folded
//! headers never draw on top of each other.

use eframe::egui::{self, Rect, RichText, Stroke, pos2, vec2};

use super::{Brand, Page, StartScreen, version_label};
use crate::theme::{self, icons, type_scale};
use crate::widgets;

/// How wide the sidebar is when open, and folded to its icons.
pub(super) const SIDEBAR_WIDTH: f32 = 220.0;
pub(super) const FOLDED_WIDTH: f32 = 64.0;
/// How long the sidebar takes to fold or open.
const SIDEBAR_SECONDS: f32 = 0.32;
/// Open chrome height (wordmark + buttons on one row).
const HEADER_OPEN: f32 = 36.0;
/// Folded chrome height (mark, then the open button under it — room so
/// neither is clipped).
const HEADER_FOLDED: f32 = 72.0;
/// The top bar's height.
pub(super) const TOP_BAR_HEIGHT: f32 = 52.0;
/// Narrower than this, the top bar's tabs drop their names (each keeps a
/// tooltip) and the version moves into the logo's tooltip.
const TOP_BAR_ROOM_FOR_NAMES: f32 = 900.0;

/// Where the start screen's navigation goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavLayout {
    /// Down the left side, like a file manager.
    #[default]
    Sidebar,
    /// Across the top, like a web app, leaving the page the full width.
    TopBar,
}

impl NavLayout {
    pub fn label(self) -> &'static str {
        match self {
            NavLayout::Sidebar => "Sidebar",
            NavLayout::TopBar => "Top bar",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            NavLayout::Sidebar => icons::SIDEBAR_SIMPLE,
            NavLayout::TopBar => icons::BROWSER,
        }
    }
}

/// The start screen's navigation as the user left it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StartNav {
    pub layout: NavLayout,
    /// The sidebar shows only its icons. The top bar ignores it.
    pub folded: bool,
}

impl StartNav {
    /// The eframe storage key it's saved under.
    pub const KEY: &'static str = "start navigation";

    pub fn to_text(self) -> String {
        match (self.layout, self.folded) {
            (NavLayout::Sidebar, false) => "sidebar",
            (NavLayout::Sidebar, true) => "sidebar folded",
            (NavLayout::TopBar, false) => "top bar",
            // Remembered for when the sidebar comes back.
            (NavLayout::TopBar, true) => "top bar, sidebar folded",
        }
        .to_owned()
    }

    /// Reads [`Self::to_text`]; anything else is the default (an open sidebar).
    pub fn from_text(text: &str) -> Self {
        let layout = if text.starts_with("top bar") {
            NavLayout::TopBar
        } else {
            NavLayout::Sidebar
        };
        Self {
            layout,
            folded: text.ends_with("folded"),
        }
    }
}

/// The pages, in the order both layouts list them: the page, its icon, its
/// name, and what its tooltip adds.
const PAGES: [(Page, &str, &str, &str); 5] = [
    (
        Page::Projects,
        icons::SQUARES_FOUR,
        "Projects",
        "Your projects, and templates to start from",
    ),
    (
        Page::Global,
        icons::GLOBE,
        "Global Assets",
        "Models, textures and sounds every project can use",
    ),
    (
        Page::Learn,
        icons::BOOK_OPEN,
        "Learn",
        "A learning path through the engine, and tutorials with sample projects",
    ),
    (
        Page::Docs,
        icons::FILE_TEXT,
        "Documentation",
        "How each part of the engine works, with search",
    ),
    (
        Page::Reference,
        icons::BOOKS,
        "API Reference",
        "Every class, property, function and event scripts can use",
    ),
];

/// "Settings (Ctrl+,)": the tooltip of the Settings button in either layout.
fn settings_tooltip(ctx: &egui::Context) -> String {
    format!(
        "Settings: theme, accent, density, layout and more ({})",
        ctx.format_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::Comma
        ))
    )
}

/// The full name, version, channel and licence, for tooltips.
fn about_line() -> String {
    format!("Axcenity Engine {}\nApache-2.0", version_label())
}

/// How far the sidebar is through folding open (1) or closed (0), eased.
/// Requests a repaint while the slide is in progress so the animation runs
/// even when the editor would otherwise sit idle between inputs.
fn sidebar_open(ctx: &egui::Context, folded: bool) -> f32 {
    let time = if ctx.global_style().animation_time <= 0.0 {
        0.0
    } else {
        SIDEBAR_SECONDS
    };
    let t = ctx.animate_bool_with_time(egui::Id::new("start sidebar open"), !folded, time);
    if time > 0.0 && t > 0.0 && t < 1.0 {
        ctx.request_repaint();
    }
    theme::bezier(theme::curve::SETTLE, t)
}

/// How strongly labels read. They stay off until the rail is wide enough
/// that titles aren't clipped against the page edge.
fn label_amount(open: f32) -> f32 {
    let t = ((open - 0.55) / 0.35).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl StartScreen {
    /// How the navigation is laid out now.
    pub fn nav(&self) -> StartNav {
        self.nav
    }

    /// Changes how the navigation is laid out (Settings, or saved settings).
    pub fn set_nav(&mut self, nav: StartNav) {
        self.nav = nav;
    }

    /// The navigation, in the layout picked, at the edge of `full`. Returns
    /// the rest of `full` for the page, and whether Settings was clicked.
    pub(super) fn navigation(
        &mut self,
        ui: &mut egui::Ui,
        full: Rect,
        tracked: usize,
        brand: &Brand,
    ) -> (Rect, bool) {
        match self.nav.layout {
            NavLayout::Sidebar => {
                let open = sidebar_open(ui.ctx(), self.nav.folded);
                let width = FOLDED_WIDTH + (SIDEBAR_WIDTH - FOLDED_WIDTH) * open;
                let rect = Rect::from_min_size(full.min, vec2(width, full.height()));
                let settings = self.sidebar(ui, rect, tracked, brand, open);
                (
                    Rect::from_min_max(pos2(rect.right(), full.top()), full.max),
                    settings,
                )
            }
            NavLayout::TopBar => {
                let rect = Rect::from_min_size(full.min, vec2(full.width(), TOP_BAR_HEIGHT));
                let settings = self.top_bar(ui, rect, tracked, brand);
                (
                    Rect::from_min_max(pos2(full.left(), rect.bottom()), full.max),
                    settings,
                )
            }
        }
    }

    /// The sidebar: width follows `open`; labels and the open header only
    /// appear once there's room. Returns true when Settings was clicked.
    fn sidebar(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        tracked: usize,
        brand: &Brand,
        open: f32,
    ) -> bool {
        let palette = theme::palette(ui.ctx());
        ui.painter().rect_filled(rect, 0, palette.app_bg);
        ui.painter().vline(
            rect.right() - 0.5,
            rect.y_range(),
            Stroke::new(1.0_f32, palette.border),
        );
        let labels = label_amount(open);
        let pad_x = 8.0 + 4.0 * open;
        let inner = rect.shrink2(vec2(pad_x, 18.0));
        let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(inner));
        // Clip to the sliding rail so nothing spills onto the page.
        bar.set_clip_rect(rect);
        let ctx = ui.ctx().clone();
        let intro = *self.intro.get_or_insert(ctx.input(|input| input.time));
        let cut = theme::arrival(&ctx, intro, 0.1, 1.0, theme::curve::CUT);
        let wipe = theme::arrival(&ctx, intro, 0.3, 0.7, theme::curve::CUT);

        self.sidebar_header(&mut bar, brand, &palette, labels, cut, wipe);

        bar.add_space(12.0 + 6.0 * open);
        bar.spacing_mut().item_spacing.y = 2.0;
        for (page, icon, title, about) in PAGES {
            let count = (page == Page::Projects).then_some(tracked);
            let selected = self.page == page;
            let tip = match count {
                Some(count) if labels < 0.5 => format!("{title} ({count})\n{about}"),
                _ if labels < 0.5 => format!("{title}\n{about}"),
                _ => about.to_owned(),
            };
            if widgets::nav_rail(&mut bar, icon, title, count, selected, open)
                .on_hover_text(tip)
                .clicked()
            {
                self.page = page;
            }
        }

        let mut top_of_version = inner.bottom();
        if labels > 0.01 {
            let mut version = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_min_max(
                        pos2(inner.left() + 10.0, inner.top()),
                        inner.max,
                    ))
                    .layout(egui::Layout::bottom_up(egui::Align::Min)),
            );
            version.set_clip_rect(rect);
            version.multiply_opacity(labels);
            version.spacing_mut().item_spacing.y = 0.0;
            let line = |text: String, color| {
                egui::Label::new(RichText::new(text).size(type_scale::CAPTION).color(color)).wrap()
            };
            version.add(line("Apache-2.0".to_owned(), palette.text_weak));
            version.add(line(version_label(), palette.text_weak));
            version.add(line("Axcenity Engine".to_owned(), palette.text_secondary));
            top_of_version = version.min_rect().top() - 8.0;
        }
        let mut foot = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    inner.min,
                    pos2(inner.right(), top_of_version),
                ))
                .layout(egui::Layout::bottom_up(egui::Align::Min)),
        );
        foot.set_clip_rect(rect);
        let settings = widgets::nav_rail(&mut foot, icons::GEAR_SIX, "Settings", None, false, open);
        settings.on_hover_text(settings_tooltip(&ctx)).clicked()
    }

    /// One header at a time: open chrome when labels are up, folded chrome
    /// (mark + open button stacked with room) when they're down. They never
    /// draw together, so the fold doesn't flash two overlapping layouts.
    fn sidebar_header(
        &mut self,
        bar: &mut egui::Ui,
        brand: &Brand,
        palette: &theme::Palette,
        labels: f32,
        cut: f32,
        wipe: f32,
    ) {
        let width = bar.available_width();
        // Prefer the open header once labels are mostly in; otherwise the
        // stacked folded chrome. A hard pick (not a dual overlay) avoids the
        // mid-slide collision that looked like a glitch.
        if labels >= 0.5 {
            let (header, _) =
                bar.allocate_exact_size(vec2(width, HEADER_OPEN), egui::Sense::hover());
            let mut open_ui = bar.new_child(
                egui::UiBuilder::new()
                    .max_rect(header)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            open_ui.set_clip_rect(header);
            open_ui.multiply_opacity(labels.max(0.001));
            open_ui.add_space(4.0);
            widgets::wordmark(&mut open_ui, brand.mark(), 15.0, cut, wipe)
                .on_hover_text(about_line());
            open_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if widgets::icon_button(ui, icons::BROWSER, "Show the pages in a top bar", false)
                    .clicked()
                {
                    self.nav.layout = NavLayout::TopBar;
                }
                if widgets::icon_button(
                    ui,
                    icons::SIDEBAR_SIMPLE,
                    "Fold the sidebar to its icons",
                    false,
                )
                .clicked()
                {
                    self.nav.folded = true;
                }
            });
            return;
        }

        // Folded: mark on top, open button under it. Height is fixed so the
        // button is never clipped by the header band.
        let (header, _) = bar.allocate_exact_size(vec2(width, HEADER_FOLDED), egui::Sense::hover());
        let fade = (1.0 - labels).clamp(0.0, 1.0);
        let mark_size = 24.0;
        let mark = Rect::from_center_size(
            pos2(header.center().x, header.top() + 6.0 + mark_size / 2.0),
            vec2(mark_size, mark_size),
        );
        let mark_response = bar.interact(mark, bar.id().with("folded mark"), egui::Sense::hover());
        widgets::paint_mark(
            bar.painter(),
            mark,
            brand.mark(),
            palette.text_strong.gamma_multiply(fade * cut.max(0.001)),
        );
        mark_response.on_hover_text(about_line());

        let button_top = mark.bottom() + 10.0;
        let button_rect = Rect::from_center_size(
            pos2(header.center().x, button_top + 14.0),
            vec2(width.min(40.0), 28.0),
        );
        // Clip to the rail, not the mark row — the button sits below the mark.
        let mut folded_ui = bar.new_child(egui::UiBuilder::new().max_rect(button_rect).layout(
            egui::Layout::centered_and_justified(egui::Direction::TopDown),
        ));
        folded_ui.set_clip_rect(header);
        folded_ui.multiply_opacity(fade);
        if widgets::icon_button(
            &mut folded_ui,
            icons::SIDEBAR_SIMPLE,
            "Open the sidebar",
            false,
        )
        .clicked()
        {
            self.nav.folded = false;
        }
    }

    /// The top bar: the logo, the pages as tabs, and at the right the
    /// version, the layout button and Settings. Narrow, the tabs show only
    /// their icons. Returns true when Settings was clicked.
    fn top_bar(&mut self, ui: &mut egui::Ui, rect: Rect, tracked: usize, brand: &Brand) -> bool {
        let palette = theme::palette(ui.ctx());
        ui.painter().rect_filled(rect, 0, palette.app_bg);
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0_f32, palette.border),
        );
        let names = rect.width() >= TOP_BAR_ROOM_FOR_NAMES;
        let inner = rect.shrink2(vec2(16.0, 0.0));
        let mut bar = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        bar.set_clip_rect(rect);
        let ctx = ui.ctx().clone();
        let intro = *self.intro.get_or_insert(ctx.input(|input| input.time));
        let cut = theme::arrival(&ctx, intro, 0.1, 1.0, theme::curve::CUT);
        let wipe = theme::arrival(&ctx, intro, 0.3, 0.7, theme::curve::CUT);
        widgets::wordmark(&mut bar, brand.mark(), 15.0, cut, wipe).on_hover_text(about_line());
        bar.add_space(28.0);
        bar.spacing_mut().item_spacing.x = 4.0;
        for (page, icon, title, about) in PAGES {
            let count = (page == Page::Projects).then_some(tracked);
            let selected = self.page == page;
            let tip = if names {
                about.to_owned()
            } else {
                format!("{title}\n{about}")
            };
            let shown = names.then_some(title);
            if widgets::nav_tab(&mut bar, icon, title, shown, count, selected, rect.bottom())
                .on_hover_text(tip)
                .clicked()
            {
                self.page = page;
            }
        }
        let mut settings = false;
        bar.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            settings = widgets::icon_button(ui, icons::GEAR_SIX, "Settings", false)
                .on_hover_text(settings_tooltip(&ctx))
                .clicked();
            if widgets::icon_button(
                ui,
                icons::SIDEBAR_SIMPLE,
                "Show the pages in a sidebar",
                false,
            )
            .clicked()
            {
                self.nav.layout = NavLayout::Sidebar;
            }
            if names {
                ui.add_space(8.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(version_label())
                            .size(type_scale::CAPTION)
                            .color(palette.text_weak),
                    )
                    .truncate(),
                )
                .on_hover_text(about_line());
            }
        });
        settings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_navigation_layout_reads_back_what_it_saves() {
        for layout in [NavLayout::Sidebar, NavLayout::TopBar] {
            for folded in [false, true] {
                let nav = StartNav { layout, folded };
                assert_eq!(StartNav::from_text(&nav.to_text()), nav);
            }
        }
        assert_eq!(StartNav::from_text(""), StartNav::default());
        assert_eq!(StartNav::from_text("sideways"), StartNav::default());
    }

    #[test]
    fn labels_stay_off_until_the_rail_is_wide_enough() {
        assert!(label_amount(0.0) < 0.01);
        assert!(label_amount(0.5) < 0.01);
        assert!(label_amount(0.7) > 0.1);
        assert!(label_amount(1.0) > 0.99);
        assert!(label_amount(0.65) < label_amount(0.85));
    }
}
