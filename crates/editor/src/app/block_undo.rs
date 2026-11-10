//! Undo and redo for block scripts, in both the Blocks and the Graph view.
//!
//! Each block script keeps its own steps, as the Behavior Tree and Dialogue
//! tabs do: while the mouse is over a block script's tab, Ctrl+Z and Ctrl+Y
//! are its, not the world's (`EditorApp::undo`).
//!
//! A step is the whole script before an edit. An edit that takes several
//! frames (dragging a stack, typing a number into a slot) is one step: it
//! starts on the first frame that changes the script and ends once no mouse
//! button is held and no text box is being typed in.

use axcenity_blocks::BlockScript;

use super::{BlockDocument, EditorApp};

/// How many steps each block script keeps.
const STEPS: usize = 100;

#[derive(Default)]
pub(super) struct BlockUndo {
    undo: Vec<BlockScript>,
    redo: Vec<BlockScript>,
    /// The script before an edit that's still going on.
    pending: Option<BlockScript>,
}

impl BlockUndo {
    /// After the editor drew a frame: `before` is the script as it was,
    /// `now` as it is. `busy` is true while a button is held or a box is
    /// typed in, so the edit may not be over.
    pub(super) fn track(&mut self, before: BlockScript, now: &BlockScript, busy: bool) {
        if *now != before && self.pending.is_none() {
            self.pending = Some(before);
        }
        if !busy {
            self.seal(now);
        }
    }

    /// Ends the edit going on, if any, as one step.
    fn seal(&mut self, now: &BlockScript) {
        let Some(before) = self.pending.take() else {
            return;
        };
        // A stack dragged back where it was is no step.
        if before == *now {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > STEPS {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Steps `script` back (or forward, for redo). Returns false when
    /// there's nothing to undo (or redo).
    pub(super) fn step(&mut self, script: &mut BlockScript, undo: bool) -> bool {
        self.seal(script);
        let (from, to) = if undo {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let Some(mut restored) = from.pop() else {
            return false;
        };
        // Which view is shown isn't an edit: undo stays in this one.
        restored.graph.shown = script.graph.shown;
        to.push(std::mem::replace(script, restored));
        true
    }

    /// Forgets every step, for a script replaced from outside the tab (its
    /// file changed on disk, or it was put back as saved).
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    #[cfg(test)]
    pub(super) fn steps(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }
}

impl EditorApp {
    /// The block script under the mouse when it was last drawn, whose own
    /// undo Ctrl+Z is.
    fn hovered_block_document(&mut self) -> Option<&mut BlockDocument> {
        let path = self.block_tab_pointer.clone()?;
        self.project
            .as_mut()?
            .block_scripts
            .iter_mut()
            .find(|document| document.path == path)
    }

    /// Undoes (or redoes) a step in the block script under the mouse.
    /// Returns false if the mouse isn't over one, so the world's undo runs
    /// instead, and true otherwise, even with nothing left to undo: Ctrl+Z
    /// over blocks never changes the world out of sight.
    pub(super) fn step_block_script(&mut self, undo: bool) -> bool {
        let Some(document) = self.hovered_block_document() else {
            return false;
        };
        if document.undo.step(&mut document.script, undo) {
            document.highlighted = None;
            let path = document.path.clone();
            self.update_block_lua(&path);
        }
        true
    }
}
