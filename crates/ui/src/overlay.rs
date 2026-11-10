//! What every screen draws over the game's own UI: the hotbar, the ability
//! bar, the quest tracker and dialogue box, the leaderboard, the chat window
//! and the ProximityPrompts, in that order, so the player and editor Play
//! always match.

use axcenity_core::ability::AbilitySlot;
use axcenity_core::dialogue::{DialogueView, QuestView};
use axcenity_core::{InstanceId, World};
use axcenity_render::{Camera, UiDrawItem};

use crate::{
    PromptView, Stylesheet, ability_bar_items, chat_items, dialogue_box_items, hotbar_items,
    leaderboard_items, prompt_items, quest_tracker_items,
};

/// One chat line: who said it (`None` for a system message) and what.
pub type ChatLine = (Option<String>, String);

/// What the overlays show, as the host gathered it this frame.
pub struct OverlayView<'a> {
    /// The player character's Inventory, whose hotbar is drawn.
    pub inventory: Option<InstanceId>,
    pub abilities: &'a [AbilitySlot],
    pub quests: &'a [QuestView],
    pub dialogue: Option<&'a DialogueView>,
    /// False while Tab has hidden the leaderboard.
    pub leaderboard: bool,
    /// The chat window's lines and the line being typed. `None` where the
    /// chat isn't drawn.
    pub chat: Option<(&'a [ChatLine], Option<&'a str>)>,
    pub prompts: &'a [PromptView],
}

/// The overlays' draw items, bottom first.
pub fn overlay_items(
    world: &World,
    view: &OverlayView,
    stylesheets: &[Stylesheet],
    camera: &Camera,
    width: u32,
    height: u32,
) -> Vec<UiDrawItem> {
    let mut items = Vec::new();
    if let Some(inventory) = view.inventory {
        items.extend(hotbar_items(world, inventory, stylesheets, width, height));
    }
    items.extend(ability_bar_items(
        world,
        view.abilities,
        stylesheets,
        width,
        height,
    ));
    items.extend(quest_tracker_items(
        world,
        view.quests,
        stylesheets,
        width,
        height,
    ));
    if let Some(dialogue) = view.dialogue {
        items.extend(dialogue_box_items(
            world,
            dialogue,
            stylesheets,
            width,
            height,
        ));
    }
    if view.leaderboard {
        items.extend(leaderboard_items(world, stylesheets, width, height));
    }
    if let Some((lines, typing)) = view.chat {
        items.extend(chat_items(world, lines, typing, stylesheets, width, height));
    }
    items.extend(prompt_items(
        world,
        view.prompts,
        stylesheets,
        camera,
        width,
        height,
    ));
    items
}
