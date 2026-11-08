//! What the dialogue box and the quest tracker show (Phase 77): the
//! `script` crate works it out, in the player's language, and `ui` draws
//! it.

/// The dialogue box: the line being said, and the choices under it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DialogueView {
    /// Who says it; empty for no name.
    pub speaker: String,
    pub text: String,
    /// The options to pick from; empty while a line waits to go on.
    pub choices: Vec<String>,
    /// The option the keyboard or controller is on.
    pub selected: usize,
}

/// One quest in the quest tracker.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuestView {
    /// Its key in the quests data table.
    pub quest: String,
    pub title: String,
    pub objectives: Vec<ObjectiveView>,
}

/// One objective of a quest: "Apples 3/5".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectiveView {
    pub text: String,
    pub progress: u32,
    pub count: u32,
}

impl ObjectiveView {
    pub fn done(&self) -> bool {
        self.progress >= self.count
    }
}
