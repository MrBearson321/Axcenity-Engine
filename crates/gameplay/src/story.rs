//! The dialogue and quest system (Phase 77, moved here by roadmap L7):
//! `DialogueService` and `QuestService` (`dialogue`, `quest`), which share
//! one state between both sides, and the frame hooks that run dialogues
//! from the player's keys and from ProximityPrompts, reach quests'
//! objectives, and fire their events.

use axcenity_core::dialogue::{DialogueView, QuestView};
use axcenity_core::{InstanceId, Value, World};
use axcenity_script::system::{self, GameSystem};
use axcenity_script::{PromptInput, ScriptRuntime, Side};
use mlua::{AnyUserData, IntoLua, Lua};

use crate::dialogue::{self, LuaDialogueService, SharedDialogues};
use crate::quest::{self, LuaQuestService, QuestEvent, SharedQuests};

/// Dialogues and quests: see the module documentation. One per runtime;
/// [`crate::install`] adds it.
#[derive(Default)]
pub struct Story {
    dialogues: SharedDialogues,
    quests: SharedQuests,
}

impl Story {
    /// Reaches the objectives the player character has got to.
    fn step_quests(&self, runtime: &ScriptRuntime, world: &World) {
        let Ok(saves) = system::saves(runtime.lua(Side::Server)) else {
            return;
        };
        let assets = runtime.assets();
        let mut saves = saves.borrow_mut();
        let mut quests = self.quests.0.borrow_mut();
        quest::QuestContext {
            saves: &mut saves,
            assets: &assets,
            events: &mut quests.events,
        }
        .step(world);
    }

    /// The player's keys for the running dialogue: go on, move between
    /// choices, or pick one by number.
    fn step_dialogue_input(
        &self,
        runtime: &mut ScriptRuntime,
        world: &mut World,
        input: &PromptInput,
    ) {
        let (presses, choices) = {
            let mut dialogues = self.dialogues.0.borrow_mut();
            let presses = dialogues.presses(&input.map, &input.game);
            let choices = dialogues.showing_choices();
            if choices {
                dialogues.move_selection(presses.step);
            }
            (presses, choices)
        };
        let side = runtime.acting_side();
        let result = runtime.run_on(side, world, |lua| match presses.number {
            Some(number) if choices => dialogue::choose(lua, number),
            _ if presses.confirm => dialogue::advance(lua),
            _ => Ok(()),
        });
        // A number with no choice of it does nothing.
        if let Err(err) = result
            && presses.number.is_none()
        {
            runtime.report_error(&err);
        }
        runtime.fire_changes(world);
    }
}

impl GameSystem for Story {
    fn services(&self) -> &'static [&'static str] {
        &["DialogueService", "QuestService"]
    }

    fn service(&self, lua: &Lua, name: &str) -> mlua::Result<mlua::Value> {
        match name {
            "DialogueService" => LuaDialogueService.into_lua(lua),
            _ => LuaQuestService.into_lua(lua),
        }
    }

    fn type_name(&self, data: &AnyUserData) -> Option<&'static str> {
        if data.is::<LuaDialogueService>() {
            Some("DialogueService")
        } else if data.is::<LuaQuestService>() {
            Some("QuestService")
        } else {
            None
        }
    }

    fn added(&self, lua: &Lua) {
        lua.set_app_data(self.dialogues.clone());
        lua.set_app_data(self.quests.clone());
    }

    fn holds_input(&self) -> bool {
        self.dialogues.0.borrow().active()
    }

    fn step_input(&self, runtime: &mut ScriptRuntime, world: &mut World, input: &PromptInput) {
        if self.holds_input() {
            self.step_dialogue_input(runtime, world, input);
        }
        self.step_quests(runtime, world);
    }

    /// A triggered prompt with a `Dialogue` starts it, with no script, the
    /// prompt's Model speaking.
    fn prompt_triggered(
        &self,
        runtime: &mut ScriptRuntime,
        world: &mut World,
        prompt: InstanceId,
        input: &PromptInput,
    ) {
        let path = world
            .get(prompt)
            .and_then(|prompt| prompt.property("Dialogue"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        if path.is_empty() || self.holds_input() {
            return;
        }
        let speaker = dialogue::speaker_of(world, prompt);
        let side = runtime.acting_side();
        let started = runtime.run_on(side, world, |lua| {
            let player = dialogue::player_of(lua);
            dialogue::start(lua, &path, speaker, player, None)
        });
        if let Err(err) = started {
            runtime.report_error(&err);
        }
        // The press that triggered it doesn't also go on.
        self.dialogues
            .0
            .borrow_mut()
            .presses(&input.map, &input.game);
        runtime.fire_changes(world);
    }

    /// Fires QuestService's events on both sides, then lets the scripts
    /// waiting on finished dialogues go on.
    fn fire_events(&self, runtime: &mut ScriptRuntime, world: &mut World) -> bool {
        let events = std::mem::take(&mut self.quests.0.borrow_mut().events);
        let finished = std::mem::take(&mut self.dialogues.0.borrow_mut().finished);
        if events.is_empty() && finished.is_empty() {
            return false;
        }
        for side in [Side::Server, Side::Client] {
            runtime.run_on(side, world, |lua| {
                fire_quest_events(lua, &events);
                dialogue::resume_finished(lua, side, &finished);
            });
            runtime.fire_change_rounds(world);
        }
        true
    }
}

/// Fires QuestService's `QuestStarted(quest)`, `QuestProgressed(quest,
/// objective, progress)` and `QuestCompleted(quest)`. The World is lent.
fn fire_quest_events(lua: &Lua, events: &[QuestEvent]) {
    const SERVICE: &str = "QuestService";
    for event in events {
        match event {
            QuestEvent::Started(quest) => system::fire(lua, SERVICE, "QuestStarted", quest.clone()),
            QuestEvent::Progressed {
                quest,
                objective,
                progress,
            } => system::fire(
                lua,
                SERVICE,
                "QuestProgressed",
                (quest.clone(), objective.clone(), *progress),
            ),
            QuestEvent::Completed(quest) => {
                system::fire(lua, SERVICE, "QuestCompleted", quest.clone());
            }
        }
    }
}

/// Whether a dialogue is running: hosts draw its box.
pub fn dialogue_active(runtime: &ScriptRuntime) -> bool {
    runtime.system::<Story>().is_some_and(Story::holds_input)
}

/// What the dialogue box shows, in the player's language.
pub fn dialogue_view(runtime: &ScriptRuntime, world: &World) -> Option<DialogueView> {
    let story = runtime.system::<Story>()?;
    dialogue::view(&story.dialogues.0.borrow(), world)
}

/// The active quests, for the quest tracker, in the player's language.
pub fn quest_tracker(runtime: &ScriptRuntime, world: &World) -> Vec<QuestView> {
    let Some(story) = runtime.system::<Story>() else {
        return Vec::new();
    };
    let Ok(saves) = system::saves(runtime.lua(Side::Server)) else {
        return Vec::new();
    };
    let assets = runtime.assets();
    let mut saves = saves.borrow_mut();
    let mut quests = story.quests.0.borrow_mut();
    quest::QuestContext {
        saves: &mut saves,
        assets: &assets,
        events: &mut quests.events,
    }
    .tracker(world)
}
