//! What each block looks like as a node: its title, its pins and the names
//! on them. The pieces come from the block's own `sections` (or a
//! reporter's `reporter_parts`), so a node always has the same inputs and
//! menus as its block; this file only names them.

use super::*;

/// The exec outputs a block's node has, with their names, top to bottom.
/// Events have one (what runs when it happens); C-shaped blocks one per
/// mouth and then Completed; blocks that never finish have no Then.
pub(super) fn exec_outs(kind: &BlockKind) -> Vec<(Out, &'static str)> {
    let mut outs = own_exec_outs(kind);
    // A function in one of its values is one more place blocks run from,
    // before Completed.
    let own = outs
        .iter()
        .filter(|(out, _)| matches!(out, Out::Body(_)))
        .count();
    let at = if matches!(outs.last(), Some((Out::Then, _))) {
        outs.len() - 1
    } else {
        outs.len()
    };
    for mouth in (own..kind.bodies().len()).rev() {
        outs.insert(at, (Out::Body(mouth), "Function"));
    }
    outs
}

fn own_exec_outs(kind: &BlockKind) -> Vec<(Out, &'static str)> {
    match kind {
        BlockKind::Unattached { .. }
        | BlockKind::TopCode { .. }
        | BlockKind::StopScript
        | BlockKind::Break
        | BlockKind::Continue
        | BlockKind::Return { .. } => Vec::new(),
        kind if kind.is_event() => vec![(Out::Body(0), "")],
        BlockKind::Repeat { .. }
        | BlockKind::RepeatUntil { .. }
        | BlockKind::While { .. }
        | BlockKind::RepeatLoop { .. }
        | BlockKind::NumericFor { .. }
        | BlockKind::GenericFor { .. } => {
            vec![(Out::Body(0), "Loop Body"), (Out::Then, "Completed")]
        }
        BlockKind::Connect { .. } => {
            vec![(Out::Body(0), "On Event"), (Out::Then, "Completed")]
        }
        BlockKind::DefineFunction { .. } | BlockKind::DoBlock { .. } => {
            vec![(Out::Body(0), "Body"), (Out::Then, "Completed")]
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
        BlockKind::WhenStreamedIn { .. } => ("When Streamed In", &["Object"]),
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
        BlockKind::WhenSplineReached { .. } => ("When Spline End Reached", &["Follower", "End"]),
        BlockKind::WhenSeatOccupied { .. } => ("When Someone Sits", &["Seat"]),
        BlockKind::WhenLayerChanged { .. } => ("When Layer Changes", &["Layer"]),
        BlockKind::WhenPropertyChanged { .. } => ("When Property Changes", &["Property", "Object"]),
        BlockKind::WhenAttributeChanged { .. } => {
            ("When Attribute Changes", &["Attribute", "Object"])
        }
        BlockKind::WhenPromptTriggered { .. } => ("When Prompt Triggered", &["Prompt"]),
        BlockKind::WhenPartClicked { .. } => ("When Part Clicked", &["Click Detector"]),
        BlockKind::WhenCommandRun { .. } => ("When Command Run", &["Command", "Help"]),
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
        BlockKind::WaitForEvent { .. } => ("Wait For Event", &["Object", "Event"]),
        BlockKind::CloneInto { .. } => ("Clone", &["Object", "Into"]),
        BlockKind::MoveTo { .. } => ("Move To", &["Part", "Position"]),
        BlockKind::MoveBy { .. } => ("Move By", &["Part", "Offset"]),
        BlockKind::LookAt { .. } => ("Look At", &["Part", "Position"]),
        BlockKind::SetCameraType { .. } => ("Set Camera Type", &["Camera Type"]),
        BlockKind::PointCameraAt { .. } => ("Point Camera At", &["Position"]),
        BlockKind::ShakeCamera { .. } => ("Shake Camera", &["Intensity", "Seconds"]),
        BlockKind::SpawnDecal { .. } => (
            "Spawn Decal",
            &["Texture", "Position", "Facing", "Size", "Seconds"],
        ),
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
        BlockKind::SetAttribute { .. } => ("Set Attribute", &["Attribute", "Object", "Value"]),
        BlockKind::PlaySound { .. } => ("Play Sound", &["Sound"]),
        BlockKind::StopSound { .. } => ("Stop Sound", &["Sound"]),
        BlockKind::PauseSound { .. } => ("Pause Sound", &["Sound"]),
        BlockKind::ResumeSound { .. } => ("Resume Sound", &["Sound"]),
        BlockKind::SetVolume { .. } => ("Set Volume", &["Sound", "Volume"]),
        BlockKind::SetAnimatorBool { .. } => ("Set Bool", &["Parameter", "Animator", "Value"]),
        BlockKind::SetAnimatorNumber { .. } => ("Set Number", &["Parameter", "Animator", "Value"]),
        BlockKind::SetAnimatorLayerWeight { .. } => (
            "Set Layer Weight",
            &["Layer", "Animator", "Weight", "Seconds"],
        ),
        BlockKind::SetAnimatorTrigger { .. } => ("Set Trigger", &["Trigger", "Animator"]),
        BlockKind::ResetAnimatorTrigger { .. } => ("Reset Trigger", &["Trigger", "Animator"]),
        BlockKind::PlayAnimatorState { .. } => ("Play State", &["State", "Animator"]),
        BlockKind::PlayAnimation { .. } => ("Play Animation", &["Animation", "Animator"]),
        BlockKind::PlayCutscene { .. } => ("Play Cutscene", &["Sequence"]),
        BlockKind::Patrol { .. } => ("Patrol", &["Agent", "Spline"]),
        BlockKind::PlaceOnSpline { .. } => ("Put On Spline", &["Object", "Spline", "Distance"]),
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
        BlockKind::RequestStreamAround { .. } => ("Load Around Player", &["Player", "Position"]),
        BlockKind::SendChatMessage { .. } => ("Send Chat Message", &["Text"]),
        BlockKind::ShowSystemMessage { .. } => ("Show System Message", &["Text"]),
        BlockKind::SetLayerActive { .. } => ("Set Layer Active", &["Layer", "Active"]),
        BlockKind::WhenConnectionFails { .. } => ("When Connection Fails", &[]),
        BlockKind::SetGraphicsQuality { .. } => ("Set Graphics Quality", &["Quality"]),
        BlockKind::SetGraphicsOption { .. } => ("Set Graphics Option", &["Setting", "Choice"]),
        BlockKind::SetGraphicsNumber { .. } => ("Set Graphics Number", &["Setting", "Value"]),
        BlockKind::Code { .. } => ("Lua", &["Code"]),
        // Their inputs vary: see `block_pin_names`.
        BlockKind::RunScript { .. } => ("Script Runs", &[]),
        BlockKind::OnSignal { .. } => ("On Signal", &["Signal", "Method", "Parameters"]),
        BlockKind::Connect { .. } => ("Connect", &["Signal", "Method", "Parameters"]),
        BlockKind::Comment { .. } => ("Comment", &["Text"]),
        BlockKind::Local { .. } => ("Local Variable", &[]),
        BlockKind::Assign { .. } => ("Set", &[]),
        BlockKind::CompoundAssign { .. } => ("Change", &["Target", "Operator", "Value"]),
        BlockKind::CallFunction { .. } => ("Call", &[]),
        BlockKind::CallMethod { .. } => ("Call Method", &[]),
        BlockKind::DefineFunction { .. } => ("Function", &["Name", "Parameters"]),
        BlockKind::While { .. } => ("While", &["Condition"]),
        BlockKind::RepeatLoop { .. } => ("Repeat Until", &["Condition"]),
        BlockKind::NumericFor { .. } => ("For", &[]),
        BlockKind::GenericFor { .. } => ("For Each", &[]),
        BlockKind::DoBlock { .. } => ("Do", &[]),
        BlockKind::Break => ("Break", &[]),
        BlockKind::Continue => ("Continue", &[]),
        BlockKind::Return { .. } => ("Return", &[]),
    }
}

const ARGUMENTS: [&str; 12] = [
    "Arg 1", "Arg 2", "Arg 3", "Arg 4", "Arg 5", "Arg 6", "Arg 7", "Arg 8", "Arg 9", "Arg 10",
    "Arg 11", "Arg 12",
];

/// `count` names for arguments.
fn arguments(count: usize) -> impl Iterator<Item = &'static str> {
    (0..count).map(|index| ARGUMENTS.get(index).copied().unwrap_or("Arg"))
}

/// The names of a block's menus and inputs, in the order its parts show them.
pub(super) fn block_pin_names(kind: &BlockKind) -> Vec<&'static str> {
    match kind {
        BlockKind::Local { values, .. } => std::iter::once("Names")
            .chain(values.iter().map(|_| "Value"))
            .collect(),
        BlockKind::Assign { targets, values } => targets
            .iter()
            .map(|_| "Target")
            .chain(values.iter().map(|_| "Value"))
            .collect(),
        BlockKind::CallFunction { args, .. } => std::iter::once("Function")
            .chain(arguments(args.len()))
            .collect(),
        BlockKind::CallMethod { args, .. } => ["Object", "Method"]
            .into_iter()
            .chain(arguments(args.len()))
            .collect(),
        BlockKind::NumericFor { step, .. } => {
            let mut names = vec!["Variable", "Start", "Stop"];
            if step.is_some() {
                names.push("Step");
            }
            names
        }
        BlockKind::GenericFor { values, .. } => std::iter::once("Names")
            .chain(values.iter().map(|_| "Values"))
            .collect(),
        BlockKind::Return { values } => values.iter().map(|_| "Value").collect(),
        kind => block_names(kind).1.to_vec(),
    }
}

/// A block's node title: a method call is named after its method.
pub(super) fn block_title(kind: &BlockKind) -> String {
    match kind {
        BlockKind::CallMethod { method, .. } => method.clone(),
        BlockKind::CallFunction {
            function: Expression::Name { name },
            ..
        } => name.clone(),
        BlockKind::Local { names, .. } => format!("Local {names}"),
        BlockKind::DefineFunction { name, .. } => format!("Function {name}"),
        kind => block_names(kind).0.to_owned(),
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
        Expression::Attribute { .. } => ("Get Attribute", &["Attribute", "Object"]),
        Expression::Raycast { .. } => ("Raycast", &["Origin", "Direction"]),
        Expression::RaycastIgnoring { .. } => {
            ("Raycast Ignoring", &["Origin", "Direction", "Ignore"])
        }
        Expression::TouchingPart => ("Touching Part", &[]),
        Expression::PartUnderMouse => ("Part Under Mouse", &[]),
        Expression::KeyDown { .. } => ("Is Key Down", &["Key"]),
        Expression::LayerActive { .. } => ("Is Layer Active", &["Layer"]),
        Expression::IsA { .. } => ("Is A", &["Object", "Class"]),
        Expression::IsStreamedIn { .. } => ("Is Streamed In", &["Object"]),
        Expression::AnimatorBool { .. } => ("Get Bool", &["Parameter", "Animator"]),
        Expression::AnimatorNumber { .. } => ("Get Number", &["Parameter", "Animator"]),
        Expression::AnimatorLayerWeight { .. } => ("Layer Weight", &["Layer", "Animator"]),
        Expression::DataRow { .. } => ("Get Row", &["Key", "Table"]),
        Expression::SplinePosition { .. } => ("Point Along Spline", &["T", "Spline"]),
        Expression::SplineClosestDistance { .. } => {
            ("Distance Along Spline", &["Spline", "Position"])
        }
        Expression::DataValue { .. } => ("Get Column", &["Column", "Row"]),
        Expression::Code { .. } => ("Lua", &["Code"]),
        Expression::Number { .. } => ("Number", &[]),
        Expression::Text { .. } => ("Text", &[]),
        Expression::Boolean { .. } => ("True or False", &[]),
        Expression::Vector3 { .. } => ("Vector3", &[]),
        Expression::Color3 { .. } => ("Color", &[]),
        Expression::Nil => ("Nil", &[]),
        Expression::Vararg => ("Arguments", &[]),
        Expression::Name { name } => return (format!("Get {name}"), &[]),
        Expression::Closure { .. } => ("Function", &[]),
        Expression::Field { name, .. } => return (format!("Get {name}"), &["Object", "Name"]),
        Expression::Index { .. } => ("Look Up", &["Table", "Key"]),
        Expression::Call {
            function: boxed, ..
        } => match &**boxed {
            Expression::Name { name } => return (name.clone(), &[]),
            _ => ("Call", &[]),
        },
        Expression::MethodCall { method, .. } => return (method.clone(), &[]),
        Expression::Table { .. } => ("Make Table", &[]),
        Expression::Binary { op, .. } => return (format!("A {op} B"), &["A", "Operator", "B"]),
        Expression::Unary { op, .. } => return (format!("{op} Value"), &["Operator", "Value"]),
        Expression::Paren { .. } => ("First Value", &["Value"]),
        Expression::Interpolated { .. } => ("Format Text", &[]),
        Expression::IfValue { .. } => ("If Value", &[]),
        Expression::Cast { .. } => ("Cast", &["Value", "Type"]),
    };
    (title.to_owned(), names)
}

/// The names of a value node's menus and inputs.
pub(super) fn value_pin_names(expression: &Expression) -> Vec<&'static str> {
    match expression {
        Expression::Call { args, .. } => std::iter::once("Function")
            .chain(arguments(args.len()))
            .collect(),
        Expression::MethodCall { args, .. } => ["Object", "Method"]
            .into_iter()
            .chain(arguments(args.len()))
            .collect(),
        Expression::Table { fields, .. } => fields
            .iter()
            .flat_map(|field| match field {
                TableField::Named { .. } => vec!["Name", "Value"],
                TableField::Keyed { .. } => vec!["Key", "Value"],
                TableField::Item { .. } => vec!["Item"],
            })
            .collect(),
        Expression::Interpolated { parts, values } => {
            let mut names = Vec::new();
            for index in 0..parts.len() {
                names.push("Text");
                if index < values.len() {
                    names.push("Value");
                }
            }
            names
        }
        Expression::IfValue { parts } => {
            let count = parts.len();
            (0..count)
                .map(|index| {
                    if index + 1 == count {
                        "Else"
                    } else if index % 2 == 0 {
                        "Condition"
                    } else {
                        "Then"
                    }
                })
                .collect()
        }
        expression => value_names(expression).1.to_vec(),
    }
}

/// The kind of value a value node gives, for its output pin's colour.
pub(super) fn value_type(expression: &Expression) -> SlotKind {
    match expression {
        _ if expression.shape() == BlockShape::Boolean
            && !expression.is_untyped()
            && !matches!(expression, Expression::Code { .. }) =>
        {
            SlotKind::Truth
        }
        Expression::Number { .. }
        | Expression::Math { .. }
        | Expression::Function { .. }
        | Expression::Random { .. }
        | Expression::RandomNumber { .. }
        | Expression::AnimatorNumber { .. }
        | Expression::AnimatorLayerWeight { .. } => SlotKind::Number,
        Expression::Text { .. } | Expression::Join { .. } | Expression::Interpolated { .. } => {
            SlotKind::Text
        }
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
fn items(parts: &[Part], names: &[&'static str], left: &mut Vec<Item>) {
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
            // Its blocks are an exec output, so it isn't wired, but it
            // counts as the slot it's in.
            Part::Params(_) => {
                left.push(Item::Field {
                    part,
                    name: named.next().unwrap_or(""),
                });
                slot += 1;
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
    items(parts, &block_pin_names(kind), &mut layout.left);
    layout.right = exec_outs(kind)
        .into_iter()
        .map(|(out, name)| Item::ExecOut(out, name))
        .collect();
    layout
}

/// A value node's rows, from its `reporter_parts`.
pub(super) fn value_layout(expression: &Expression, parts: &[Part]) -> Layout {
    let mut layout = Layout::default();
    items(parts, &value_pin_names(expression), &mut layout.left);
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
