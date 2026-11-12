//! What every block, reporter and graph node is for, in words (cohesion
//! pass CW4.1): the one catalogue behind the tooltips of the Blocks
//! palette and canvas, the Graph view's node menu and the node-graph
//! editors (UI Event Graphs, behavior trees, dialogues).
//!
//! Blocks and reporters are described by an exhaustive `match`, so a new
//! [`BlockKind`] or [`Expression`] doesn't build until it's described here.
//! Graph nodes keep their words in their [`NodeSpec`]s.

use crate::node_graph::{NodeSpec, PinType};
use crate::{BlockKind, Expression};

/// What a block, reporter or node is, for a tooltip.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeInfo {
    /// Its name, such as `When touched`.
    pub title: &'static str,
    /// What it does, in a sentence or two.
    pub description: &'static str,
    /// What it's for, as an example of use. Empty when the description says it.
    pub example: &'static str,
    /// Its inputs: each one's name and what it takes.
    pub inputs: Vec<(String, String)>,
    /// What it gives back, for a reporter or a node with outputs.
    pub output: Option<String>,
}

/// The catalogue entry for a statement or event block.
pub fn block_info(kind: &BlockKind) -> NodeInfo {
    let (title, description, example) = block_words(kind);
    NodeInfo {
        title,
        description,
        example,
        inputs: inputs_of(serde_json::to_value(kind).ok()),
        output: None,
    }
}

/// The catalogue entry for a reporter (a value block).
pub fn expression_info(expression: &Expression) -> NodeInfo {
    let (title, description, example, output) = expression_words(expression);
    NodeInfo {
        title,
        description,
        example,
        inputs: inputs_of(serde_json::to_value(expression).ok()),
        output: Some(output.to_owned()),
    }
}

/// The catalogue entry for a node of a node graph, from its spec.
pub fn node_info(spec: &NodeSpec) -> NodeInfo {
    let pin = |name: &str, ty: PinType| {
        let what = match ty {
            PinType::Exec => "what runs next",
            PinType::Number => "a number",
            PinType::Text => "text",
            PinType::Bool => "true or false",
            PinType::Vector2 => "two numbers",
            PinType::Vector3 => "three numbers",
            PinType::Any => "any value",
        };
        (name.to_owned(), what.to_owned())
    };
    let outputs: Vec<String> = spec
        .outputs
        .iter()
        .filter(|output| output.ty != PinType::Exec)
        .map(|output| {
            let (name, what) = pin(output.name, output.ty);
            format!("{name}: {what}")
        })
        .collect();
    NodeInfo {
        title: spec.title,
        description: spec.help,
        example: "",
        inputs: spec
            .inputs
            .iter()
            .filter(|input| input.ty != PinType::Exec)
            .map(|input| pin(input.name, input.ty))
            .chain(spec.fields.iter().map(|field| {
                (
                    field.name.to_owned(),
                    field_help(field.name)
                        .unwrap_or("typed on the node")
                        .to_owned(),
                )
            }))
            .collect(),
        output: (!outputs.is_empty()).then(|| outputs.join(", ")),
    }
}

/// The inputs of a block or reporter: each field it has, but its stacks
/// of blocks, with what it takes.
fn inputs_of(value: Option<serde_json::Value>) -> Vec<(String, String)> {
    let Some(serde_json::Value::Object(fields)) = value else {
        return Vec::new();
    };
    fields
        .keys()
        .filter(|name| !matches!(name.as_str(), "type" | "multiline") && !name.ends_with("body"))
        .map(|name| {
            let help = field_help(name).unwrap_or("what goes in this slot");
            (name.replace('_', " "), help.to_owned())
        })
        .collect()
}

/// What an input of a block, reporter or node takes, by its name. `None`
/// for a name the catalogue doesn't know (a test checks every palette
/// block's are known).
pub fn field_help(name: &str) -> Option<&'static str> {
    Some(match name {
        "target" => "the object it acts on, picked from the tree",
        "part" => "the part, picked from the tree",
        "key" => "a key on the keyboard, or a value to look up by",
        "button" => "the TextButton, picked from the tree",
        "element" => "the UI element, picked from the tree",
        "widget" => "the Slider, Toggle, Dropdown or ProgressBar",
        "event" => "the event's name or path",
        "sound" => "the Sound, picked from the tree",
        "sequence" => "the Sequence (cutscene), picked from the tree",
        "animator" => "the Animator, picked from the tree",
        "agent" => "the NavigationAgent, picked from the tree",
        "sense" => "seeing or hearing",
        "follower" => "the SplineFollower, picked from the tree",
        "which" => "which end of the spline",
        "seat" => "the Seat, picked from the tree",
        "prompt" => "the ProximityPrompt, picked from the tree",
        "detector" => "the ClickDetector, picked from the tree",
        "command" => "the word typed in the Developer Console",
        "help" => "the line the console's help shows for it",
        "code" => "Lua kept word for word",
        "seconds" => "how long, in seconds",
        "condition" => "a true-or-false value (a pointed block)",
        "count" => "how many times",
        "variable" => "the name each one is kept under while the blocks inside run",
        "center" => "a position (Vector3)",
        "radius" => "how far from the center, in studs",
        "name" => "a name",
        "position" => "a position in the world (Vector3)",
        "offset" => "how far to move on each axis (Vector3)",
        "distance" => "how far, in studs",
        "orientation" => "degrees around each axis (Vector3)",
        "rotation" => "degrees to turn around each axis (Vector3)",
        "anchored" => "true keeps it in place; false lets physics move it",
        "force" => "the push on each axis (Vector3)",
        "velocity" => "studs a second on each axis (Vector3)",
        "friction" => "0 is ice, 1 is rubber",
        "color" => "a color",
        "text" => "text to show",
        "layout" => "the PageLayout, picked from the tree",
        "page" => "the name of the page's Frame",
        "x" | "y" | "z" => "a number for that axis",
        "r" | "g" | "b" => "how much red, green or blue, 0 to 1",
        "value" => "the value to use",
        "property" => "the name of a property, such as Transparency",
        "controller" => "the character controller, picked from the tree",
        "slot" => "the save slot's number or name",
        "checkpoint" => "the Checkpoint, picked from the tree",
        "level" => "the level's name",
        "spawn" => "where characters appear (empty: a SpawnLocation)",
        "port" => "the UDP port others join on",
        "address" => "the host's address and port, such as 192.168.1.20:47800",
        "owner" => "a Player, or nil for the server",
        "player" => "a Player",
        "reason" => "what the player is told",
        "camera_type" => "Scriptable (scripts move it) or Custom (the controller does)",
        "intensity" => "how hard it shakes",
        "texture" => "the picture",
        "facing" => "the way the surface faces (Vector3)",
        "size" => "how big, in studs",
        "language" => "a language code, such as en or fr",
        "curve" => "the tone mapping curve",
        "time" => "the time of day, in hours (0 to 24)",
        "quality" => "a graphics quality level",
        "setting" => "the graphics setting",
        "choice" => "the setting's choice",
        "volume" => "how loud, 0 to 10 (1 is as recorded)",
        "parameter" => "the state machine parameter's name",
        "state" => "the state's name",
        "layer" => "the state machine layer's name",
        "weight" => "how much it counts, 0 to 1",
        "animation" => "the Animation, picked from the tree",
        "spline" => "the Spline, picked from the tree",
        "style" => "the easing curve, such as Quad",
        "direction" => "how the easing runs (In, Out, InOut), or a direction (Vector3)",
        "from" => "where it starts",
        "by" => "how much to add (negative takes away)",
        "parent" => "where the copy goes (empty: the Workspace)",
        "signal" => "the event to listen to, such as part.Touched",
        "method" => "Connect, Once or ConnectParallel",
        "params" => "the names the event's values are kept under",
        "names" => "the names to keep the values under",
        "values" => "the values, in order",
        "targets" => "what to set, in order",
        "op" => "the operator",
        "function" => "the function to call",
        "args" => "what to pass it, in order",
        "local" => "whether it's local to this script",
        "generics" => "its generic type names (Luau)",
        "returns" => "the type it returns (Luau)",
        "attributes" => "attributes written before it, such as @native",
        "start" => "the first number",
        "stop" => "the last number",
        "step" => "how much to add each time (1 when left out)",
        "min" => "the smallest it can be",
        "max" => "the largest it can be",
        "to" => "where it ends",
        "alpha" => "how far along, 0 to 1",
        "left" => "the first value",
        "right" => "the second value",
        "origin" => "where the ray starts (Vector3)",
        "ignore" => "what the ray passes through, with everything inside it",
        "class" => "a class name, such as BasePart",
        "table" => "the data table's name",
        "column" => "the column's name",
        "row" => "a row of a data table",
        "t" => "how far along, 0 (start) to 1 (end)",
        "object" => "the value it's on",
        "fields" => "the table's entries",
        "parts" => "the text around the values",
        "annotation" => "the type, as written",
        _ => return None,
    })
}

type Words = (&'static str, &'static str, &'static str);

/// Title, description and example of each block.
fn block_words(kind: &BlockKind) -> Words {
    use BlockKind as B;
    match kind {
        // Events.
        B::WhenStarted { .. } => (
            "When the game starts",
            "Runs the blocks inside once, as soon as the game starts.",
            "Set the score to 0 and print a welcome.",
        ),
        B::WhenTouched { .. } => (
            "When touched",
            "Runs the blocks inside each time something touches the part. The touching part is the \"touching part\" reporter.",
            "Hurt whoever steps on a lava floor.",
        ),
        B::WhenStreamedIn { .. } => (
            "When streamed in",
            "Runs when world streaming brings the object (or a Model's contents) back onto this device.",
            "Start a far-away windmill turning once it's near.",
        ),
        B::WhenKeyPressed { .. } => (
            "When key pressed",
            "Runs each time the player presses the key. LocalScripts only.",
            "Open the map when M is pressed.",
        ),
        B::WhenClicked { .. } => (
            "When button clicked",
            "Runs each time the TextButton is clicked or tapped. LocalScripts only.",
            "Start the game from a Play button.",
        ),
        B::WhenMouseEnters { .. } => (
            "When pointer enters",
            "Runs when the mouse pointer comes onto the UI element. LocalScripts only.",
            "Highlight a menu item under the pointer.",
        ),
        B::WhenMouseLeaves { .. } => (
            "When pointer leaves",
            "Runs when the mouse pointer goes off the UI element. LocalScripts only.",
            "Put a highlighted menu item back.",
        ),
        B::WhenValueChanged { .. } => (
            "When value changes",
            "Runs when a Slider, Toggle, Dropdown or ProgressBar changes, by the player or a script. Read the new value from its Value.",
            "Set the music volume from a slider.",
        ),
        B::EveryFrame { .. } => (
            "Every frame",
            "Runs the blocks inside every frame, after physics.",
            "Keep a part spinning, a little each frame.",
        ),
        B::BeforePhysics { .. } => (
            "Before physics",
            "Runs the blocks inside every frame, just before physics moves things.",
            "Push a hovering platform before it falls.",
        ),
        B::BeforeRender { .. } => (
            "Before drawing",
            "Runs the blocks inside every frame, just before it's drawn. LocalScripts only.",
            "Move the camera to follow something smoothly.",
        ),
        B::WhenEventFired { .. } => (
            "When event fired",
            "Runs each time the BindableEvent fires, from any script.",
            "Open every door when the \"alarm\" event fires.",
        ),
        B::WhenSoundEnded { .. } => (
            "When sound ends",
            "Runs when the Sound finishes playing.",
            "Play the next song when one ends.",
        ),
        B::WhenCutsceneFinished { .. } => (
            "When cutscene finishes",
            "Runs when the Sequence reaches its end, or is skipped there.",
            "Give the player control back after the intro.",
        ),
        B::WhenConnectionFails { .. } => (
            "When connection fails",
            "Runs when hosting or joining fails, a join is refused, or the connection is lost.",
            "Show a \"Connection lost\" message.",
        ),
        B::WhenStateChanged { .. } => (
            "When animation state changes",
            "Runs when the Animator's state machine moves to another state.",
            "Play footsteps when the character starts running.",
        ),
        B::WhenAnimationEvent { .. } => (
            "When animation event",
            "Runs when the Animator's clip passes the named event.",
            "Play a swoosh at the \"swing\" event of an attack.",
        ),
        B::WhenAgentSenses { .. } => (
            "When agent senses",
            "Runs when the NavigationAgent starts seeing or hearing the player character.",
            "Make a guard chase the player it sees.",
        ),
        B::WhenSplineReached { .. } => (
            "When spline end reached",
            "Runs when a SplineFollower gets to the start or end of its spline.",
            "Turn a train round at the end of the line.",
        ),
        B::WhenSeatOccupied { .. } => (
            "When seat taken",
            "Runs when a character sits on the Seat.",
            "Start a ride when someone sits down.",
        ),
        B::WhenPromptTriggered { .. } => (
            "When prompt triggered",
            "Runs when the player uses the ProximityPrompt: presses its key, or holds it long enough.",
            "Open a chest with \"Press E to open\".",
        ),
        B::WhenPartClicked { .. } => (
            "When part clicked",
            "Runs when the ClickDetector's part is clicked.",
            "Ring a doorbell when it's clicked.",
        ),
        B::WhenCommandRun { .. } => (
            "When console command",
            "Adds a command to the Developer Console and runs the blocks inside when it's typed.",
            "A \"heal\" command for testing.",
        ),
        B::Unattached { .. } => (
            "Loose blocks",
            "Blocks under no event. They don't run until they're put under one.",
            "",
        ),
        B::TopCode { .. } | B::Code { .. } => (
            "Lua",
            "Lua that has no block of its own, kept word for word. It runs where it is, exactly as typed.",
            "",
        ),
        B::RunScript { .. } => (
            "Script top",
            "The statements at the top of the script, run once in order when it starts.",
            "",
        ),
        B::OnSignal { .. } => (
            "On event",
            "Runs the blocks inside each time the event fires (signal:Connect).",
            "React to any event, such as a part's Changed.",
        ),
        B::Connect { .. } => (
            "Connect",
            "Starts listening to the event from here: the blocks inside run each time it fires.",
            "Listen to a part made by the script.",
        ),
        // Control.
        B::Wait { .. } => (
            "Wait",
            "Pauses these blocks for some seconds. The rest of the game keeps going.",
            "Wait 3 seconds between waves.",
        ),
        B::WaitUntil { .. } => (
            "Wait until",
            "Pauses these blocks until the condition is true.",
            "Wait until the door is open.",
        ),
        B::Repeat { .. } => (
            "Repeat",
            "Runs the blocks inside a number of times.",
            "Spawn 10 coins.",
        ),
        B::Forever { .. } => (
            "Forever",
            "Runs the blocks inside again and again. Put a wait inside, or it never lets anything else run.",
            "Blink a light on and off.",
        ),
        B::RepeatUntil { .. } => (
            "Repeat until",
            "Runs the blocks inside again and again until the condition is true.",
            "Move a platform until it reaches the top.",
        ),
        B::If { .. } => (
            "If",
            "Runs the blocks inside only when the condition is true.",
            "Only open the door if the player has the key.",
        ),
        B::IfElse { .. } => (
            "If else",
            "Runs the first blocks when the condition is true, the second when it isn't.",
            "Say \"You win\" or \"Try again\".",
        ),
        B::StopScript => (
            "Stop this script",
            "Ends the event's blocks here; nothing after it runs this time.",
            "Stop when the game is already over.",
        ),
        B::ForEachChild { .. } => (
            "For each child",
            "Runs the blocks inside once for each child of the object, with it in the variable.",
            "Color every part in a Model.",
        ),
        B::ForEachPartInRadius { .. } => (
            "For each part nearby",
            "Runs the blocks inside once for each part within the radius of a point, with it in the variable.",
            "Knock back everything near an explosion.",
        ),
        B::ForEachPartTouching { .. } => (
            "For each part touching",
            "Runs the blocks inside once for each part overlapping the object, such as a trigger zone.",
            "Count the players standing on a pad.",
        ),
        B::WaitForChild { .. } => (
            "Wait for child",
            "Pauses until the object has a child with that name.",
            "Wait for the character's Humanoid.",
        ),
        B::DestroyAfter { .. } => (
            "Destroy after",
            "Removes the object after some seconds, without waiting for it.",
            "Clear a spent bullet after 5 seconds.",
        ),
        // Motion.
        B::MoveTo { .. } => (
            "Move to",
            "Puts the object at a position.",
            "Send the player back to the start.",
        ),
        B::MoveBy { .. } => (
            "Move by",
            "Moves the object by an offset from where it is.",
            "Lift a door 10 studs.",
        ),
        B::LookAt { .. } => (
            "Look at",
            "Turns the object to face a position.",
            "Make a turret face the player.",
        ),
        B::MoveForward { .. } => (
            "Move forward",
            "Moves the object along the way it faces.",
            "Drive a car forward a little each frame.",
        ),
        B::TurnTo { .. } => (
            "Turn to",
            "Sets which way the object faces, in degrees around each axis.",
            "Stand a fallen sign back up.",
        ),
        B::TurnBy { .. } => (
            "Turn by",
            "Turns the object by some degrees from how it faces now.",
            "Spin a coin a few degrees each frame.",
        ),
        B::SetAnchored { .. } => (
            "Set anchored",
            "Anchored parts stay where they are; unanchored ones fall and are pushed by physics.",
            "Drop a bridge when its switch is pressed.",
        ),
        B::PushWithForce { .. } => (
            "Push",
            "Gives the part a push (an impulse): heavier parts move less.",
            "A jump pad that launches whoever steps on it.",
        ),
        B::SetPartVelocity { .. } => (
            "Set velocity",
            "Sets how fast the part moves, whatever its mass.",
            "Throw a ball at a set speed.",
        ),
        B::SetFriction { .. } => (
            "Set friction",
            "Sets how much the part grips what it slides on, keeping its other physical properties.",
            "Make a floor icy.",
        ),
        // Looks.
        B::SetColor { .. } => (
            "Set color",
            "Sets the object's color.",
            "Turn a light green when a level is done.",
        ),
        B::SetText { .. } => (
            "Set text",
            "Sets the text a label, button or text box shows.",
            "Show the score.",
        ),
        B::Show { .. } => (
            "Show",
            "Makes the object or UI element visible.",
            "Show the pause menu.",
        ),
        B::Hide { .. } => (
            "Hide",
            "Makes the object or UI element invisible.",
            "Hide a collected coin.",
        ),
        B::ShowPage { .. } => (
            "Show page",
            "Turns a PageLayout to one of its pages.",
            "Go to the Settings page of a menu.",
        ),
        B::ScrollTo { .. } => (
            "Scroll to",
            "Scrolls a ScrollingFrame to a place, in pixels.",
            "Scroll a list back to the top.",
        ),
        B::SetValue { .. } => (
            "Set value",
            "Sets a Slider, Toggle, Dropdown or ProgressBar, which also fires its value change.",
            "Fill a health bar.",
        ),
        B::SetProperty { .. } => (
            "Set property",
            "Sets any property of an object by its name.",
            "Set a part's Transparency to 0.5.",
        ),
        // Characters and agents.
        B::SetDestination { .. } => (
            "Walk to",
            "Walks a NavigationAgent to a position, finding its way around walls.",
            "Send a guard to the alarm.",
        ),
        B::LeaveSeat { .. } => (
            "Leave seat",
            "Stands the character up from its seat.",
            "Throw the driver out of a crashed car.",
        ),
        B::SetVelocity { .. } => (
            "Move agent at",
            "Moves a NavigationAgent straight at a speed, with no path. Zero stops it.",
            "Make a pet dash ahead.",
        ),
        // Saving and levels.
        B::SaveSlot { .. } => (
            "Save game",
            "Writes a save slot: player data, the world, the active checkpoint and settings.",
            "Save when the player reaches a camp.",
        ),
        B::LoadSlot { .. } => (
            "Load game",
            "Puts the game back as the save slot was saved.",
            "A Continue button.",
        ),
        B::SetCheckpoint { .. } => (
            "Set checkpoint",
            "Makes the Checkpoint the active one, capturing the game there.",
            "A flag the player touches halfway through.",
        ),
        B::RestoreCheckpoint => (
            "Back to checkpoint",
            "Returns to the active checkpoint, with the player standing on it.",
            "Respawn after falling into a pit.",
        ),
        B::LoadLevel { .. } => (
            "Load level",
            "Replaces the level with another, with characters at a spawn there.",
            "Go to level 2 through a portal.",
        ),
        B::AddLevel { .. } => (
            "Add level",
            "Copies a level into the world beside the current one.",
            "Load the next room before the player gets there.",
        ),
        B::RemoveLevel { .. } => (
            "Remove level",
            "Takes an added level out again.",
            "Unload a room the player has left.",
        ),
        // Multiplayer.
        B::HostGame { .. } => (
            "Host game",
            "Hosts the game so other players can join.",
            "A Host button on the main menu.",
        ),
        B::JoinGame { .. } => (
            "Join game",
            "Joins a game someone else is hosting.",
            "A Join button with an address box.",
        ),
        B::LeaveGame => (
            "Leave game",
            "Leaves the game joined, or stops hosting.",
            "A Quit to Menu button.",
        ),
        B::SetNetworkOwner { .. } => (
            "Set network owner",
            "Lets one player (or the server) simulate the part's physics.",
            "Let the driver simulate their car so it feels instant.",
        ),
        B::SetNetworkOwnershipAuto { .. } => (
            "Network owner: automatic",
            "Lets the engine pick who simulates the part: the nearest player.",
            "Hand a car back when its driver gets out.",
        ),
        B::KickPlayer { .. } => (
            "Kick player",
            "Takes a player out of the game, telling them why.",
            "Remove a player the host votes out.",
        ),
        B::RequestStreamAround { .. } => (
            "Stream around",
            "Loads the world around a position for a player, and waits for it.",
            "Load the arena before teleporting a player there.",
        ),
        B::SendChatMessage { .. } => (
            "Send chat message",
            "Sends text to the chat as this player. LocalScripts only.",
            "A quick-chat \"Good game!\" button.",
        ),
        B::ShowSystemMessage { .. } => (
            "Show system message",
            "Shows text in the chat as the game's own message.",
            "Announce \"Round 2 begins\".",
        ),
        // Camera and world.
        B::SetCameraType { .. } => (
            "Set camera type",
            "Scriptable leaves the camera to scripts; Custom gives it back to the player's controller.",
            "Take the camera for a cutscene.",
        ),
        B::PointCameraAt { .. } => (
            "Point camera at",
            "Turns the camera to look at a position from where it is.",
            "Look at a boss as it appears.",
        ),
        B::ShakeCamera { .. } => (
            "Shake camera",
            "Shakes the camera for some seconds.",
            "Shake on an explosion.",
        ),
        B::SpawnDecal { .. } => (
            "Spawn decal",
            "Lays a picture on a surface, which fades out after some seconds.",
            "Leave a scorch mark where a rocket hit.",
        ),
        B::SetLanguage { .. } => (
            "Set language",
            "Sets the language the game's translated text shows in.",
            "A language picker in Settings.",
        ),
        B::SetToneMapping { .. } => (
            "Set tone mapping",
            "Sets how the picture's brightness is mapped to the screen.",
            "A filmic look for a cutscene.",
        ),
        B::SetClockTime { .. } => (
            "Set time of day",
            "Sets the time of day, which moves the sun and moon.",
            "Make it night when the boss arrives.",
        ),
        B::SetAtmosphere { .. } => (
            "Set atmosphere",
            "Sets one of the Atmosphere's numbers, such as its density (fog).",
            "Thicken the fog in a swamp.",
        ),
        B::SetGraphicsQuality { .. } => (
            "Set graphics quality",
            "Sets the player's graphics quality preset.",
            "A Low / Medium / High picker.",
        ),
        B::SetGraphicsOption { .. } => (
            "Set graphics option",
            "Sets one graphics setting, or puts it back to its preset's.",
            "Turn shadows off in a menu.",
        ),
        B::SetGraphicsNumber { .. } => (
            "Set graphics number",
            "Sets a numeric graphics setting, such as render scale.",
            "A field-of-view slider.",
        ),
        // Sound.
        B::PlaySound { .. } => (
            "Play sound",
            "Plays the Sound from the start.",
            "A coin pickup sound.",
        ),
        B::StopSound { .. } => ("Stop sound", "Stops the Sound.", "Stop the alarm."),
        B::PauseSound { .. } => (
            "Pause sound",
            "Pauses the Sound where it is.",
            "Pause the music on the pause menu.",
        ),
        B::ResumeSound { .. } => (
            "Resume sound",
            "Plays a paused Sound on from where it stopped.",
            "Unpause the music.",
        ),
        B::SetVolume { .. } => (
            "Set volume",
            "Sets how loud the Sound plays.",
            "Turn the music down during dialogue.",
        ),
        // Animation.
        B::SetAnimatorBool { .. } => (
            "Set animator true/false",
            "Sets a true-or-false parameter of the Animator's state machine.",
            "Set \"Crouching\" while a key is held.",
        ),
        B::SetAnimatorNumber { .. } => (
            "Set animator number",
            "Sets a number parameter of the Animator's state machine.",
            "Set \"Speed\" from how fast the character moves.",
        ),
        B::SetAnimatorTrigger { .. } => (
            "Fire animator trigger",
            "Fires a trigger of the Animator's state machine, once.",
            "Fire \"Jump\".",
        ),
        B::ResetAnimatorTrigger { .. } => (
            "Reset animator trigger",
            "Clears a trigger that hasn't been used yet.",
            "Cancel a queued attack.",
        ),
        B::PlayAnimatorState { .. } => (
            "Play animator state",
            "Moves the Animator's state machine straight to a state.",
            "Jump to \"Dead\".",
        ),
        B::SetAnimatorLayerWeight { .. } => (
            "Set layer weight",
            "Fades how much a state machine layer counts over some seconds.",
            "Blend in an aiming layer.",
        ),
        B::PlayAnimation { .. } => (
            "Play animation",
            "Loads an Animation on the Animator and plays it.",
            "Wave when the player says hello.",
        ),
        B::PlayCutscene { .. } => (
            "Play cutscene",
            "Plays a Sequence: its camera, keys, sounds and subtitles.",
            "Play the intro when the game starts.",
        ),
        B::PlaceOnSpline { .. } => (
            "Place on spline",
            "Puts an object some distance along a Spline, facing along it.",
            "Move a cart along a track.",
        ),
        B::Patrol { .. } => (
            "Patrol",
            "Has a NavigationAgent walk a Spline's points in order.",
            "A guard walking its rounds.",
        ),
        B::Tween { .. } => (
            "Tween",
            "Moves one property smoothly to a value over some seconds, eased.",
            "Slide a door open over 1 second.",
        ),
        B::SlideIn { .. } => (
            "Slide in",
            "Slides a UI element in from past an edge to where it is.",
            "Slide a menu in from the left.",
        ),
        B::FadeIn { .. } => (
            "Fade in",
            "Fades a UI element's background in.",
            "Fade in a dialog box.",
        ),
        B::FadeOut { .. } => (
            "Fade out",
            "Fades a UI element's background out.",
            "Fade a hint away.",
        ),
        // Variables and objects.
        B::SetVariable { .. } => (
            "Set variable",
            "Keeps a value under a name, to use later.",
            "Set score to 0.",
        ),
        B::ChangeVariable { .. } => (
            "Change variable",
            "Adds to a number variable (a negative number takes away).",
            "Add 1 to score for each coin.",
        ),
        B::Print { .. } => (
            "Print",
            "Writes a value to the Output panel, to see what's happening.",
            "Print the player's health while testing.",
        ),
        B::Destroy { .. } => (
            "Destroy",
            "Removes the object and everything in it from the game.",
            "Remove a collected coin.",
        ),
        B::CloneInto { .. } => (
            "Clone into",
            "Copies the object and puts the copy somewhere.",
            "Spawn enemies from a template in ServerStorage.",
        ),
        B::FireEvent { .. } => (
            "Fire event",
            "Fires a BindableEvent, running every script listening to it.",
            "Tell every door that the alarm went off.",
        ),
        // Lua as blocks.
        B::Comment { .. } => (
            "Comment",
            "A note for people reading the script. It doesn't run.",
            "",
        ),
        B::Local { .. } => (
            "Local",
            "Makes new names in this part of the script and gives them values (local).",
            "local speed = 16",
        ),
        B::Assign { .. } => (
            "Set",
            "Gives existing names or properties new values.",
            "part.Transparency = 1",
        ),
        B::CompoundAssign { .. } => (
            "Change by",
            "Changes a value with an operator, such as adding to it (+=).",
            "score += 10",
        ),
        B::CallFunction { .. } => (
            "Call function",
            "Calls a function with some values.",
            "print(\"hi\")",
        ),
        B::CallMethod { .. } => (
            "Call method",
            "Calls a method on an object (object:method).",
            "part:Destroy()",
        ),
        B::DefineFunction { .. } => (
            "Define function",
            "Makes a function: named blocks to run from elsewhere with some values.",
            "A function that heals a player by an amount.",
        ),
        B::While { .. } => (
            "While",
            "Runs the blocks inside again and again while the condition is true.",
            "while alive do ... end",
        ),
        B::RepeatLoop { .. } => (
            "Repeat until (Lua)",
            "Runs the blocks inside, then again until the condition is true.",
            "repeat ... until done",
        ),
        B::NumericFor { .. } => (
            "For",
            "Counts from a number to another, running the blocks inside with each.",
            "for i = 1, 10 do ... end",
        ),
        B::GenericFor { .. } => (
            "For in",
            "Runs the blocks inside once for each item a list or table gives.",
            "for _, part in parts do ... end",
        ),
        B::DoBlock { .. } => (
            "Do",
            "Groups blocks so the names made inside stay inside.",
            "",
        ),
        B::Break => (
            "Break",
            "Leaves the loop it's in at once.",
            "Stop searching once found.",
        ),
        B::Continue => (
            "Continue",
            "Skips to the next turn of the loop it's in.",
            "Skip parts that are anchored.",
        ),
        B::Return { .. } => (
            "Return",
            "Ends the function, giving back values.",
            "return total",
        ),
    }
}

/// Title, description, example and what it gives back, of each reporter.
fn expression_words(
    expression: &Expression,
) -> (&'static str, &'static str, &'static str, &'static str) {
    use Expression as E;
    match expression {
        E::Number { .. } => ("Number", "A number typed in.", "", "a number"),
        E::Text { .. } => ("Text", "Text typed in.", "", "text"),
        E::Boolean { .. } => ("True / false", "true or false.", "", "true or false"),
        E::Vector3 { .. } => (
            "Vector3",
            "Three numbers: a position, an offset, a direction or a size.",
            "0, 10, 0 is 10 studs up.",
            "a Vector3",
        ),
        E::Color3 { .. } => (
            "Color",
            "A color, from red, green and blue.",
            "",
            "a Color3",
        ),
        E::Variable { .. } => (
            "Variable",
            "The value kept under a name.",
            "",
            "the variable's value",
        ),
        E::Math { .. } => (
            "Math",
            "Adds, takes away, multiplies, divides or raises two numbers.",
            "score + 10",
            "a number",
        ),
        E::Function { .. } => (
            "Math function",
            "Rounds a number, or gives its absolute value, square root and so on.",
            "Round a timer to whole seconds.",
            "a number",
        ),
        E::Random { .. } => (
            "Random whole number",
            "A random whole number from the smallest to the largest, both included.",
            "Roll a die: 1 to 6.",
            "a whole number",
        ),
        E::RandomNumber { .. } => (
            "Random number",
            "Any random number from the smallest up to the largest.",
            "A random delay of 0.5 to 2 seconds.",
            "a number",
        ),
        E::Lerp { .. } => (
            "Blend",
            "The position or color some way from one to another (0 is the first, 1 the second).",
            "A color halfway between red and blue.",
            "a Vector3 or Color3",
        ),
        E::Join { .. } => (
            "Join",
            "Two values as text, one after the other.",
            "\"Score: \" joined with score.",
            "text",
        ),
        E::Compare { .. } => (
            "Compare",
            "Whether two values are equal, or one is less or more than the other.",
            "health < 1",
            "true or false",
        ),
        E::Logic { .. } => (
            "And / or",
            "and: both are true. or: at least one is.",
            "has key and door is closed",
            "true or false",
        ),
        E::Not { .. } => (
            "Not",
            "true when the value is false, and false when it's true.",
            "",
            "true or false",
        ),
        E::Property { .. } => (
            "Property",
            "A property of an object, such as a part's Position.",
            "The door's Position.",
            "the property's value",
        ),
        E::Raycast { .. } => (
            "Raycast",
            "Casts a ray and gives what it hits first (nil if nothing): where, which part and its surface.",
            "Is there ground under the player?",
            "a RaycastResult, or nil",
        ),
        E::RaycastIgnoring { .. } => (
            "Raycast ignoring",
            "A raycast that passes through an object and everything in it.",
            "Shoot without hitting your own character.",
            "a RaycastResult, or nil",
        ),
        E::TouchingPart => (
            "Touching part",
            "The part that touched, inside \"when touched\".",
            "Check if it belongs to a player.",
            "a part",
        ),
        E::PartUnderMouse => (
            "Part under mouse",
            "The part under the mouse pointer. LocalScripts only.",
            "Pick a unit to select.",
            "a part, or nil",
        ),
        E::KeyDown { .. } => (
            "Key down",
            "Whether a key is held down now. LocalScripts only.",
            "Run while Shift is held.",
            "true or false",
        ),
        E::AnimatorBool { .. } => (
            "Animator true/false",
            "A true-or-false parameter of the Animator's state machine.",
            "",
            "true or false",
        ),
        E::AnimatorNumber { .. } => (
            "Animator number",
            "A number parameter of the Animator's state machine.",
            "",
            "a number",
        ),
        E::AnimatorLayerWeight { .. } => (
            "Layer weight",
            "How much a state machine layer counts now.",
            "",
            "a number, 0 to 1",
        ),
        E::IsA { .. } => (
            "Is a",
            "Whether an object is of a class, or one that comes from it.",
            "Only push parts: is a BasePart.",
            "true or false",
        ),
        E::IsStreamedIn { .. } => (
            "Is streamed in",
            "Whether an object is on this device now (world streaming).",
            "",
            "true or false",
        ),
        E::DataRow { .. } => (
            "Data table row",
            "A row of a data table, by its key. nil when there's none.",
            "The \"sword\" row of weapons.",
            "a row, or nil",
        ),
        E::SplinePosition { .. } => (
            "Point on spline",
            "The point some way along a Spline (0 its start, 1 its end).",
            "",
            "a Vector3",
        ),
        E::SplineClosestDistance { .. } => (
            "Distance along spline",
            "How far along a Spline its point nearest a position is.",
            "How far along the track a racer is.",
            "a number of studs",
        ),
        E::DataValue { .. } => (
            "Row value",
            "A row's value in one column.",
            "The Damage of the sword row.",
            "the value in that column",
        ),
        E::Nil => ("nil", "Nothing: no value.", "", "nil"),
        E::Vararg => (
            "...",
            "The extra values a function was given.",
            "",
            "the values",
        ),
        E::Name { .. } => (
            "Name",
            "A name in the script, such as a variable or a service.",
            "",
            "its value",
        ),
        E::Field { .. } => (
            "Field",
            "A field of a value (object.name).",
            "",
            "the field's value",
        ),
        E::Index { .. } => (
            "Index",
            "An entry of a table by its key (object[key]).",
            "",
            "the entry",
        ),
        E::Call { .. } => (
            "Call",
            "Calls a function and gives what it returns.",
            "",
            "what it returns",
        ),
        E::MethodCall { .. } => (
            "Method call",
            "Calls a method on an object and gives what it returns.",
            "",
            "what it returns",
        ),
        E::Closure { .. } => (
            "Function",
            "A function made here, to pass to something else.",
            "",
            "a function",
        ),
        E::Table { .. } => (
            "Table",
            "A list or a table of values ({ ... }).",
            "",
            "a table",
        ),
        E::Binary { .. } => (
            "Operator",
            "Two values with an operator between them.",
            "",
            "the result",
        ),
        E::Unary { .. } => (
            "Operator",
            "A value with -, # (length) or not before it.",
            "",
            "the result",
        ),
        E::Paren { .. } => ("Brackets", "A value in brackets.", "", "the value"),
        E::Interpolated { .. } => (
            "Text with values",
            "Text with values written into it (`Score: {score}`).",
            "",
            "text",
        ),
        E::IfValue { .. } => (
            "If value",
            "One value or another, by a condition (if a then b else c).",
            "",
            "one of the values",
        ),
        E::Cast { .. } => (
            "Type cast",
            "A value with its type stated (Luau ::).",
            "",
            "the value",
        ),
        E::Code { .. } => (
            "Lua",
            "Lua that has no block of its own, kept word for word.",
            "",
            "its value",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_graph::NodeLibrary;
    use crate::{behavior_tree, dialogue, ui_graph};

    /// Every graph node says what it does, and its pins are listed.
    #[test]
    fn every_graph_node_has_words_and_its_pins() {
        let libraries: [&dyn NodeLibrary; 3] = [
            &ui_graph::UiNodes,
            &behavior_tree::BehaviorNodes,
            &dialogue::DialogueNodes,
        ];
        for library in libraries {
            for spec in library.specs() {
                let info = node_info(spec);
                assert!(!info.description.is_empty(), "{} says nothing", spec.kind);
                let typed = spec
                    .inputs
                    .iter()
                    .filter(|pin| pin.ty != PinType::Exec)
                    .count();
                assert_eq!(
                    info.inputs.len(),
                    typed + spec.fields.len(),
                    "{}",
                    spec.kind
                );
            }
        }
    }

    #[test]
    fn a_blocks_card_lists_its_inputs_but_not_its_stacks() {
        let info = block_info(&BlockKind::Repeat {
            count: Expression::Number { value: 3.0 },
            body: Vec::new(),
        });
        assert_eq!(info.title, "Repeat");
        assert_eq!(
            info.inputs,
            [("count".to_owned(), "how many times".to_owned())]
        );
        let info = expression_info(&Expression::Random {
            min: Box::new(Expression::Number { value: 1.0 }),
            max: Box::new(Expression::Number { value: 6.0 }),
        });
        assert_eq!(info.output.as_deref(), Some("a whole number"));
        assert_eq!(info.inputs.len(), 2);
    }
}
