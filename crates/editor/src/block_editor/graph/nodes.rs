//! What each block looks like as a node: its title, its pins and the names
//! on them. The pieces come from the block's own `sections` (or a
//! reporter's `reporter_parts`), so a node always has the same inputs and
//! menus as its block; this file only names them.

use super::*;

/// The exec outputs a block's node has, with their names, top to bottom.
/// Events have one (what runs when it happens); C-shaped blocks one per
/// mouth and then Completed; blocks that never finish have no Then.
pub(super) fn exec_outs(kind: &BlockKind) -> Vec<(Out, &'static str)> {
    match kind {
        BlockKind::Unattached { .. } | BlockKind::TopCode { .. } | BlockKind::StopScript => {
            Vec::new()
        }
        kind if kind.is_event() => vec![(Out::Body(0), "")],
        BlockKind::Repeat { .. } | BlockKind::RepeatUntil { .. } => {
            vec![(Out::Body(0), "Loop Body"), (Out::Then, "Completed")]
        }
        BlockKind::Forever { .. } => vec![(Out::Body(0), "Loop Body")],
        // An if is a Branch whose False isn't wired yet.
        BlockKind::If { .. } | BlockKind::IfElse { .. } => vec![
            (Out::Body(0), "True"),
            (Out::Body(1), "False"),
            (Out::Then, "Completed"),
        ],
        _ => vec![(Out::Then, "")],
    }
}

/// Whether a block's node has an exec input: every block in a stack.
pub(super) fn has_exec_in(kind: &BlockKind) -> bool {
    !kind.is_event()
}

/// A block's node title, and the names of its menus and inputs in the
/// order its parts show them (labels and icons aside).
pub(super) fn block_names(kind: &BlockKind) -> (&'static str, &'static [&'static str]) {
    match kind {
        BlockKind::WhenStarted { .. } => ("When Game Starts", &[]),
        BlockKind::WhenTouched { .. } => ("When Touched", &["Part"]),
        BlockKind::WhenKeyPressed { .. } => ("When Key Pressed", &["Key"]),
        BlockKind::WhenClicked { .. } => ("When Clicked", &["Button"]),
        BlockKind::WhenMouseEnters { .. } => ("When Mouse Enters", &["Element"]),
        BlockKind::WhenMouseLeaves { .. } => ("When Mouse Leaves", &["Element"]),
        BlockKind::WhenValueChanged { .. } => ("When Value Changes", &["Widget"]),
        BlockKind::EveryFrame { .. } => ("Every Frame", &[]),
        BlockKind::BeforePhysics { .. } => ("Before Physics", &[]),
        BlockKind::BeforeRender { .. } => ("Before Drawing", &[]),
        BlockKind::WhenEventFired { .. } => ("When Event Fires", &["Event"]),
        BlockKind::FireEvent { .. } => ("Fire Event", &["Event"]),
        BlockKind::DestroyAfter { .. } => ("Destroy After", &["Object", "Seconds"]),
        BlockKind::WhenSoundEnded { .. } => ("When Sound Ends", &["Sound"]),
        BlockKind::WhenCutsceneFinished { .. } => ("When Cutscene Finishes", &["Sequence"]),
        BlockKind::WhenStateChanged { .. } => ("When State Changes", &["Animator"]),
        BlockKind::WhenAnimationEvent { .. } => ("When Animation Event", &["Animator", "Event"]),
        BlockKind::WhenAgentSenses { .. } => ("When Agent Senses Player", &["Agent", "Sense"]),
        BlockKind::WhenSeatOccupied { .. } => ("When Someone Sits", &["Seat"]),
        BlockKind::WhenPromptTriggered { .. } => ("When Prompt Triggered", &["Prompt"]),
        BlockKind::WhenPartClicked { .. } => ("When Part Clicked", &["Click Detector"]),
        BlockKind::Unattached { .. } => ("Loose Blocks", &[]),
        BlockKind::TopCode { .. } => ("Lua (runs first)", &["Code"]),
        BlockKind::Wait { .. } => ("Wait", &["Seconds"]),
        BlockKind::WaitUntil { .. } => ("Wait Until", &["Condition"]),
        BlockKind::Repeat { .. } => ("Repeat", &["Times"]),
        BlockKind::Forever { .. } => ("Forever", &[]),
        BlockKind::RepeatUntil { .. } => ("Repeat Until", &["Condition"]),
        BlockKind::If { .. } | BlockKind::IfElse { .. } => ("Branch", &["Condition"]),
        BlockKind::StopScript => ("Stop This Script", &[]),
        BlockKind::ForEachChild { .. } => ("For Each Child", &["Variable", "Object"]),
        BlockKind::ForEachPartInRadius { .. } => {
            ("For Each Part In Radius", &["Variable", "Radius", "Center"])
        }
        BlockKind::ForEachPartTouching { .. } => ("For Each Part Touching", &["Variable", "Part"]),
        BlockKind::WaitForChild { .. } => ("Wait For Child", &["Name", "Object"]),
        BlockKind::CloneInto { .. } => ("Clone", &["Object", "Into"]),
        BlockKind::MoveTo { .. } => ("Move To", &["Part", "Position"]),
        BlockKind::MoveBy { .. } => ("Move By", &["Part", "Offset"]),
        BlockKind::LookAt { .. } => ("Look At", &["Part", "Position"]),
        BlockKind::SetCameraType { .. } => ("Set Camera Type", &["Camera Type"]),
        BlockKind::PointCameraAt { .. } => ("Point Camera At", &["Position"]),
        BlockKind::ShakeCamera { .. } => ("Shake Camera", &["Intensity", "Seconds"]),
        BlockKind::MoveForward { .. } => ("Move Forward", &["Part", "Distance"]),
        BlockKind::TurnTo { .. } => ("Turn To", &["Part", "Orientation"]),
        BlockKind::TurnBy { .. } => ("Turn By", &["Part", "Rotation"]),
        BlockKind::SetAnchored { .. } => ("Set Anchored", &["Part", "Anchored"]),
        BlockKind::PushWithForce { .. } => ("Push", &["Part", "Force"]),
        BlockKind::SetPartVelocity { .. } => ("Set Part Velocity", &["Part", "Velocity"]),
        BlockKind::SetFriction { .. } => ("Set Friction", &["Part", "Friction"]),
        BlockKind::SetDestination { .. } => ("Walk To", &["Agent", "Destination"]),
        BlockKind::SetVelocity { .. } => ("Set Velocity", &["Agent", "Velocity"]),
        BlockKind::LeaveSeat { .. } => ("Leave Seat", &["Controller"]),
        BlockKind::SetColor { .. } => ("Set Color", &["Part", "Color"]),
        BlockKind::SetText { .. } => ("Set Text", &["Object", "Text"]),
        BlockKind::Show { .. } => ("Show", &["Object"]),
        BlockKind::Hide { .. } => ("Hide", &["Object"]),
        BlockKind::ShowPage { .. } => ("Show Page", &["Page", "Layout"]),
        BlockKind::ScrollTo { .. } => ("Scroll To", &["Frame", "X", "Y"]),
        BlockKind::SetValue { .. } => ("Set Value", &["Widget", "Value"]),
        BlockKind::SetProperty { .. } => ("Set Property", &["Property", "Object", "Value"]),
        BlockKind::PlaySound { .. } => ("Play Sound", &["Sound"]),
        BlockKind::StopSound { .. } => ("Stop Sound", &["Sound"]),
        BlockKind::PauseSound { .. } => ("Pause Sound", &["Sound"]),
        BlockKind::ResumeSound { .. } => ("Resume Sound", &["Sound"]),
        BlockKind::SetVolume { .. } => ("Set Volume", &["Sound", "Volume"]),
        BlockKind::SetAnimatorBool { .. } => ("Set Bool", &["Parameter", "Animator", "Value"]),
        BlockKind::SetAnimatorNumber { .. } => ("Set Number", &["Parameter", "Animator", "Value"]),
        BlockKind::SetAnimatorTrigger { .. } => ("Set Trigger", &["Trigger", "Animator"]),
        BlockKind::ResetAnimatorTrigger { .. } => ("Reset Trigger", &["Trigger", "Animator"]),
        BlockKind::PlayAnimatorState { .. } => ("Play State", &["State", "Animator"]),
        BlockKind::PlayAnimation { .. } => ("Play Animation", &["Animation", "Animator"]),
        BlockKind::PlayCutscene { .. } => ("Play Cutscene", &["Sequence"]),
        BlockKind::Tween { .. } => (
            "Tween",
            &[
                "Property",
                "Object",
                "Value",
                "Seconds",
                "Style",
                "Direction",
            ],
        ),
        BlockKind::SlideIn { .. } => ("Slide In", &["Object", "From", "Seconds"]),
        BlockKind::FadeIn { .. } => ("Fade In", &["Object", "Seconds"]),
        BlockKind::FadeOut { .. } => ("Fade Out", &["Object", "Seconds"]),
        BlockKind::SetVariable { .. } => ("Set Variable", &["Variable", "Value"]),
        BlockKind::ChangeVariable { .. } => ("Change Variable", &["Variable", "By"]),
        BlockKind::Print { .. } => ("Print", &["Value"]),
        BlockKind::Destroy { .. } => ("Destroy", &["Object"]),
        BlockKind::SaveSlot { .. } => ("Save Slot", &["Slot"]),
        BlockKind::LoadSlot { .. } => ("Load Slot", &["Slot"]),
        BlockKind::SetCheckpoint { .. } => ("Set Checkpoint", &["Checkpoint"]),
        BlockKind::RestoreCheckpoint => ("Restore Checkpoint", &[]),
        BlockKind::SetToneMapping { .. } => ("Set Tone Mapping", &["Tone Mapping"]),
        BlockKind::SetClockTime { .. } => ("Set Time of Day", &["Hours"]),
        BlockKind::SetAtmosphere { .. } => ("Set Atmosphere", &["Property", "Value"]),
        BlockKind::SetLanguage { .. } => ("Set Language", &["Language"]),
        BlockKind::LoadLevel { .. } => ("Load Level", &["Level", "Spawn"]),
        BlockKind::AddLevel { .. } => ("Add Level", &["Level"]),
        BlockKind::RemoveLevel { .. } => ("Remove Level", &["Level"]),
        BlockKind::HostGame { .. } => ("Host Game", &["Port"]),
        BlockKind::JoinGame { .. } => ("Join Game", &["Address"]),
        BlockKind::LeaveGame => ("Leave Game", &[]),
        BlockKind::SetNetworkOwner { .. } => ("Set Network Owner", &["Owner", "Part"]),
        BlockKind::SetNetworkOwnershipAuto { .. } => ("Network Ownership Auto", &["Part"]),
        BlockKind::KickPlayer { .. } => ("Kick Player", &["Player", "Reason"]),
        BlockKind::WhenConnectionFails { .. } => ("When Connection Fails", &[]),
        BlockKind::SetGraphicsQuality { .. } => ("Set Graphics Quality", &["Quality"]),
        BlockKind::SetGraphicsOption { .. } => ("Set Graphics Option", &["Setting", "Choice"]),
        BlockKind::SetGraphicsNumber { .. } => ("Set Graphics Number", &["Setting", "Value"]),
        BlockKind::Code { .. } => ("Lua", &["Code"]),
    }
}

/// A value node's title, and the names of its menus and inputs.
pub(super) fn value_names(expression: &Expression) -> (String, &'static [&'static str]) {
    let (title, names): (&str, &'static [&'static str]) = match expression {
        // A variable's node is named after it, as in Unreal's getters.
        Expression::Variable { name } => return (format!("Get {name}"), &[]),
        Expression::Math { .. } => ("Math", &["A", "Operator", "B"]),
        Expression::Function { .. } => ("Math Function", &["Function", "Value"]),
        Expression::Random { .. } => ("Random Integer", &["Min", "Max"]),
        Expression::RandomNumber { .. } => ("Random Number", &["Min", "Max"]),
        Expression::Lerp { .. } => ("Lerp", &["From", "To", "Alpha"]),
        Expression::Join { .. } => ("Join Text", &["A", "B"]),
        Expression::Compare { .. } => ("Compare", &["A", "Operator", "B"]),
        Expression::Logic { .. } => ("And / Or", &["A", "Operator", "B"]),
        Expression::Not { .. } => ("Not", &["Value"]),
        Expression::Property { .. } => ("Get Property", &["Property", "Object"]),
        Expression::Raycast { .. } => ("Raycast", &["Origin", "Direction"]),
        Expression::RaycastIgnoring { .. } => {
            ("Raycast Ignoring", &["Origin", "Direction", "Ignore"])
        }
        Expression::TouchingPart => ("Touching Part", &[]),
        Expression::PartUnderMouse => ("Part Under Mouse", &[]),
        Expression::KeyDown { .. } => ("Is Key Down", &["Key"]),
        Expression::IsA { .. } => ("Is A", &["Object", "Class"]),
        Expression::AnimatorBool { .. } => ("Get Bool", &["Parameter", "Animator"]),
        Expression::AnimatorNumber { .. } => ("Get Number", &["Parameter", "Animator"]),
        Expression::DataRow { .. } => ("Get Row", &["Key", "Table"]),
        Expression::DataValue { .. } => ("Get Column", &["Column", "Row"]),
        Expression::Code { .. } => ("Lua", &["Code"]),
        Expression::Number { .. } => ("Number", &[]),
        Expression::Text { .. } => ("Text", &[]),
        Expression::Boolean { .. } => ("True or False", &[]),
        Expression::Vector3 { .. } => ("Vector3", &[]),
        Expression::Color3 { .. } => ("Color", &[]),
    };
    (title.to_owned(), names)
}

/// The kind of value a value node gives, for its output pin's colour.
pub(super) fn value_type(expression: &Expression) -> SlotKind {
    match expression {
        _ if expression.shape() == BlockShape::Boolean
            && !matches!(expression, Expression::Code { .. }) =>
        {
            SlotKind::Truth
        }
        Expression::Number { .. }
        | Expression::Math { .. }
        | Expression::Function { .. }
        | Expression::Random { .. }
        | Expression::RandomNumber { .. }
        | Expression::AnimatorNumber { .. } => SlotKind::Number,
        Expression::Text { .. } | Expression::Join { .. } => SlotKind::Text,
        Expression::Vector3 { .. } => SlotKind::Vector,
        Expression::Color3 { .. } => SlotKind::Color,
        _ => SlotKind::Any,
    }
}

/// Unreal-like pin colours: green numbers, pink text, red true or false,
/// gold vectors, blue colours, grey for anything.
pub(super) fn pin_color(kind: SlotKind) -> Color32 {
    match kind {
        SlotKind::Number => Color32::from_rgb(140, 220, 70),
        SlotKind::Text => Color32::from_rgb(240, 90, 210),
        SlotKind::Condition | SlotKind::Truth => Color32::from_rgb(220, 50, 50),
        SlotKind::Vector => Color32::from_rgb(245, 195, 40),
        SlotKind::Color => Color32::from_rgb(70, 140, 255),
        SlotKind::Any => Color32::from_rgb(170, 170, 178),
    }
}

/// One row item on a node's left (inputs) or right (outputs) side.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Item {
    ExecIn,
    ExecOut(Out, &'static str),
    /// Value slot `slot` (in slot order), fed by a wire or typed in.
    Input {
        slot: usize,
        part: usize,
        kind: SlotKind,
        name: &'static str,
    },
    /// A menu on the node (an object, a key, an operator), as part `part`.
    Field {
        part: usize,
        name: &'static str,
    },
    /// Grey Lua, shown across the node.
    Code {
        part: usize,
    },
    Output(SlotKind),
}

/// A node's rows: what's on the left and on the right of each.
#[derive(Clone, Debug, Default)]
pub(super) struct Layout {
    pub(super) left: Vec<Item>,
    pub(super) right: Vec<Item>,
}

impl Layout {
    pub(super) fn rows(&self) -> usize {
        self.left.len().max(self.right.len())
    }
}

/// The items for `parts` named by `names`, after the exec input if any.
fn items(parts: &[Part], names: &'static [&'static str], left: &mut Vec<Item>) {
    let mut named = names.iter().copied();
    let mut slot = 0;
    for (part, item) in parts.iter().enumerate() {
        match item {
            Part::Label(_) | Part::Icon(_) => {}
            Part::Slot(_, kind) => {
                left.push(Item::Input {
                    slot,
                    part,
                    kind: *kind,
                    name: named.next().unwrap_or(""),
                });
                slot += 1;
            }
            Part::Code(_) => {
                named.next();
                left.push(Item::Code { part });
            }
            _ => left.push(Item::Field {
                part,
                name: named.next().unwrap_or(""),
            }),
        }
    }
}

/// A block's rows, from its parts (`sections` with the mouths left out).
pub(super) fn block_layout(kind: &BlockKind, parts: &[Part]) -> Layout {
    let mut layout = Layout::default();
    if has_exec_in(kind) {
        layout.left.push(Item::ExecIn);
    }
    items(parts, block_names(kind).1, &mut layout.left);
    layout.right = exec_outs(kind)
        .into_iter()
        .map(|(out, name)| Item::ExecOut(out, name))
        .collect();
    layout
}

/// A value node's rows, from its `reporter_parts`.
pub(super) fn value_layout(expression: &Expression, parts: &[Part]) -> Layout {
    let mut layout = Layout::default();
    items(parts, value_names(expression).1, &mut layout.left);
    layout.right.push(Item::Output(value_type(expression)));
    layout
}

/// A block's parts on its rows, with its mouths left out (they're pins).
pub(super) fn block_parts<'a>(kind: &'a mut BlockKind, objects: &[ObjectInfo]) -> Vec<Part<'a>> {
    sections(kind, objects)
        .into_iter()
        .flat_map(|section| match section {
            Section::Row(parts) => parts,
            Section::Mouth(_) => Vec::new(),
        })
        .collect()
}
