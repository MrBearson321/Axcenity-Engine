//! The Team Board's place in the app (the panel is `team_board.rs`;
//! `docs/editor/team-board.md`): the open project's board, read from
//! `team/board/` and written back an item at a time. In a Collaborate
//! session the mirror shares each written file like any save, and files
//! that arrive are read again through `disk_changed`; the project's
//! repository keeps the board's history with everything else.

use std::collections::BTreeSet;
use std::path::PathBuf;

use axcenity_collab::Capability;
use axcenity_teamwork::{Board, Copies, Item, time};
use eframe::egui;

use super::EditorApp;
use crate::team_board::{BoardAction, BoardPanel, Context};

/// The board of the open project, and the panel's state.
#[derive(Default)]
pub(crate) struct TeamBoard {
    pub(crate) panel: BoardPanel,
    /// The project folder the board was read from, and the board.
    loaded: Option<(PathBuf, Board)>,
    /// A file of the board changed since it was read.
    pub(crate) stale: bool,
    /// This computer's person, read once.
    pub(crate) me: Option<String>,
    /// Items already assigned to this computer's person when last looked,
    /// so a new assignment says so once.
    assigned: Option<BTreeSet<String>>,
}

impl TeamBoard {
    /// The board, if one is read.
    pub(crate) fn board(&self) -> Option<&Board> {
        self.loaded.as_ref().map(|(_, board)| board)
    }
}

impl EditorApp {
    /// This computer's person on the board: the Collaborate name.
    pub(crate) fn board_me(&mut self) -> String {
        if let Some(me) = &self.team_board.me {
            return me.clone();
        }
        let me = super::team::load_own_identity()
            .map(|identity| identity.name)
            .unwrap_or_else(|_| "Me".to_owned());
        self.team_board.me = Some(me.clone());
        me
    }

    /// Whether this editor may change the board: there's a project, and in
    /// a session its role may change files.
    pub(in crate::app) fn board_writable(&self) -> bool {
        self.project.is_some()
            && self
                .team
                .as_ref()
                .and_then(|team| team.client_role())
                .is_none_or(|role| role.can(Capability::EditAssets))
    }

    /// Copies two people's changes left are merged by one computer: not a
    /// member's copy of a session (the host merges and shares the result),
    /// and not while the repository waits for someone to decide a conflict.
    fn board_copies(&self) -> Copies {
        let member = self.in_team_session()
            && self
                .team
                .as_ref()
                .is_some_and(|team| team.hosting().is_none());
        let deciding = matches!(
            self.repo.state(),
            Some(axcenity_repo::sync::State::Conflict(_))
        );
        if member || deciding {
            Copies::Leave
        } else {
            Copies::Merge
        }
    }

    /// Reads the open project's board when it isn't read yet, or a file of
    /// it changed.
    pub(crate) fn load_board(&mut self) {
        let Some(root) = self.project.as_ref().map(|p| p.folder.clone()) else {
            self.team_board.loaded = None;
            return;
        };
        let current = self
            .team_board
            .loaded
            .as_ref()
            .is_some_and(|(folder, _)| *folder == root);
        if current && !self.team_board.stale {
            return;
        }
        if !current {
            self.team_board.assigned = None;
        }
        self.team_board.stale = false;
        let (board, loaded) = Board::load(&root, self.board_copies());
        for problem in &board.problems {
            log::warn!("Team Board: {problem}");
        }
        if !loaded.merged.is_empty() {
            self.output.info(format!(
                "Team Board: merged two people's changes to {} item(s).",
                loaded.merged.len()
            ));
        }
        self.team_board.loaded = Some((root, board));
        self.notice_new_assignments();
    }

    /// A file of the board changed on disk (another program, or a session
    /// or repository bringing someone's change): read it again.
    pub(in crate::app) fn board_file_changed(&mut self) {
        self.team_board.stale = true;
    }

    /// Says once when someone else assigned this computer's person an item.
    fn notice_new_assignments(&mut self) {
        let me = self.board_me();
        let Some(board) = self.team_board.board() else {
            return;
        };
        let mine: BTreeSet<String> = board
            .items()
            .filter(|item| item.is_assigned(&me) && !item.status.is_finished())
            .map(|item| item.id.clone())
            .collect();
        let new: Vec<&Item> = match &self.team_board.assigned {
            Some(before) => mine
                .iter()
                .filter(|id| !before.contains(*id))
                .filter_map(|id| board.get(id))
                // Not the ones this computer's person took themselves.
                .filter(|item| !axcenity_teamwork::item::same_person(&item.updated_by, &me))
                .collect(),
            None => Vec::new(),
        };
        let texts: Vec<String> = new
            .iter()
            .map(|item| format!("{} assigned you “{}”", item.updated_by, item.title))
            .collect();
        self.team_board.assigned = Some(mine);
        for text in texts {
            self.toasts.success(text.clone());
            self.output.info(format!("Team Board: {text}"));
        }
    }

    /// The people who can be assigned: the board's, the session's and this
    /// computer's.
    fn board_people(&mut self) -> Vec<String> {
        let mut others = vec![self.board_me()];
        if let Some(team) = &self.team {
            others.extend(team.roster().iter().map(|c| c.profile.name.clone()));
        }
        self.team_board
            .board()
            .map(|board| board.people(&others))
            .unwrap_or(others)
    }

    /// Draws the Team Board panel.
    pub(in crate::app) fn team_board_pane(&mut self, ui: &mut egui::Ui) {
        self.load_board();
        if self.project.is_none() {
            crate::widgets::empty_state(
                ui,
                crate::theme::icons::KANBAN,
                "No project open",
                "Open a project to see its Team Board.",
                None,
            );
            return;
        }
        let me = self.board_me();
        let people = self.board_people();
        let selected = self.selected.and_then(|id| self.world.full_name(id));
        let script = self.tabs.active().map(|tab| tab.path.clone());
        let read_only = !self.board_writable();
        let mut panel = std::mem::take(&mut self.team_board.panel);
        let Some(board) = self.team_board.board() else {
            self.team_board.panel = panel;
            return;
        };
        let cx = Context {
            board,
            me: &me,
            people: &people,
            now: time::now(),
            selected,
            script,
            read_only,
        };
        let actions = panel.show(ui, &cx);
        self.team_board.panel = panel;
        self.apply_board_actions(actions);
    }

    /// Each frame: reads the board again if a file changed, and draws the
    /// open item's window.
    pub(in crate::app) fn team_board_frame(&mut self, ctx: &egui::Context) {
        if self.team_board.stale || self.team_board.panel.open.is_some() {
            self.load_board();
        }
        if self.team_board.panel.open.is_none() {
            return;
        }
        let me = self.board_me();
        let people = self.board_people();
        let selected = self.selected.and_then(|id| self.world.full_name(id));
        let script = self.tabs.active().map(|tab| tab.path.clone());
        let read_only = !self.board_writable();
        let mut panel = std::mem::take(&mut self.team_board.panel);
        let Some(board) = self.team_board.board() else {
            panel.open = None;
            self.team_board.panel = panel;
            return;
        };
        let cx = Context {
            board,
            me: &me,
            people: &people,
            now: time::now(),
            selected,
            script,
            read_only,
        };
        let actions = panel.window(ctx, &cx);
        self.team_board.panel = panel;
        self.apply_board_actions(actions);
    }

    pub(in crate::app) fn apply_board_actions(&mut self, actions: Vec<BoardAction>) {
        for action in actions {
            self.apply_board_action(action);
        }
    }

    fn apply_board_action(&mut self, action: BoardAction) {
        self.load_board();
        let writable = self.board_writable();
        let me = self.board_me();
        let Some((root, board)) = self.team_board.loaded.as_mut() else {
            return;
        };
        let root = root.clone();
        let result = match action {
            BoardAction::Create { kind, title } if writable => {
                let item = Item::new(kind, &title, &me, time::now());
                if item.title.is_empty() {
                    return;
                }
                let opened = item.clone();
                let result = board.save(&root, item);
                if result.is_ok() {
                    self.team_board.panel.open_item(&opened);
                }
                result
            }
            BoardAction::Save(item) if writable => board.save(&root, *item),
            BoardAction::Delete(id) if writable => board.delete(&root, &id),
            BoardAction::SelectInstance(path) => {
                match crate::console::resolve_path(&self.world, &path) {
                    Some(id) => self.select_many(vec![id]),
                    None => self
                        .output
                        .warning(format!("Team Board: there's no `{path}` in the world now.")),
                }
                return;
            }
            BoardAction::OpenFile { path, line } => {
                self.open_script(&path, line.map(|l| l as usize));
                return;
            }
            // A Viewer can look but not change.
            BoardAction::Create { .. } | BoardAction::Save(_) | BoardAction::Delete(_) => return,
        };
        if let Err(error) = result {
            self.output
                .error(format!("Team Board: couldn't save the board: {error}"));
        }
        self.notice_new_assignments();
    }

    /// The Collaborate panel's line about the board: how many items are
    /// open and yours, and a button that opens it.
    pub(in crate::app) fn board_summary_ui(&mut self, ui: &mut egui::Ui) {
        if self.project.is_none() {
            return;
        }
        self.load_board();
        let me = self.board_me();
        let Some(board) = self.team_board.board() else {
            return;
        };
        let palette = crate::theme::palette(ui.ctx());
        let today = axcenity_teamwork::Date::of(time::now());
        let open = board
            .items()
            .filter(|item| !item.status.is_finished())
            .count();
        let (mine, late) = board.open_for(&me, today);
        let mut text = format!("{open} open");
        if mine > 0 {
            text.push_str(&format!(" · {mine} yours"));
        }
        if late > 0 {
            text.push_str(&format!(" · {late} overdue"));
        }
        let mut show = false;
        ui.horizontal(|ui| {
            ui.label(crate::theme::with_icon(
                crate::theme::icons::KANBAN,
                "Team Board",
            ));
            ui.label(egui::RichText::new(text).color(if late > 0 {
                palette.error
            } else {
                palette.text_weak
            }));
            show = ui.small_button("Open").clicked();
        });
        if show {
            self.layout.open(crate::layout::Pane::TeamBoard);
        }
        ui.add_space(crate::theme::space::S);
    }
}
