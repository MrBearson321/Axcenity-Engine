//! Scratch-style script data and its one-way translation to Lua: what a
//! game needs at runtime, so the player can load block scripts and UI
//! Event Graphs without the block editor's tools (those, such as
//! `from_lua`, are in `axcenity-blocks`, which re-exports this crate).
//!
//! A [`BlockScript`] is a canvas of stacks. Each stack starts with an event
//! block (a "hat", such as "when game starts") whose body runs when the
//! event happens, or is an [`BlockKind::Unattached`] stack of loose blocks
//! that doesn't run, like blocks left lying around in Scratch. Statement
//! blocks hold values in slots: plain values typed in (numbers, text, ...)
//! or reporter blocks ([`Expression`]) that work a value out.
//!
//! Every block translates to the documented Lua API (see `docs/scripting/blocks.md`),
//! so a block script runs exactly like a typed one. The Graph view shows
//! the same blocks as nodes and wires, laid out by [`Graph`].

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod behavior_tree;
pub mod catalogue;
pub mod dialogue;
pub mod node_graph;
mod translate;
pub mod ui_graph;

pub use translate::*;

static NEXT_BLOCK_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockShape {
    /// Starts a stack ("when ..."): only at the top level.
    Event,
    /// One step in a stack.
    Statement,
    /// A round reporter: a number, text or other value.
    Value,
    /// A pointed reporter: true or false.
    Boolean,
}

impl BlockShape {
    /// Whether a block of shape `child` fits where a block of shape `self`
    /// goes. As in Scratch, a true-or-false reporter also fits a value slot,
    /// but only true-or-false reporters fit a condition.
    pub fn accepts(self, child: BlockShape) -> bool {
        matches!(
            (self, child),
            (BlockShape::Event, BlockShape::Event)
                | (BlockShape::Statement, BlockShape::Statement)
                | (BlockShape::Value, BlockShape::Value | BlockShape::Boolean)
                | (BlockShape::Boolean, BlockShape::Boolean)
        )
    }
}

/// Which side of the game a block script runs on: blocks that read the
/// keyboard or UI clicks only work on the player's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Server,
    Client,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockScript {
    pub blocks: Vec<Block>,
    /// How the script looks as a node graph. Written only once it's been
    /// shown as one, so other block scripts don't change.
    #[serde(default, skip_serializing_if = "Graph::is_empty")]
    pub graph: Graph,
    /// Indent the Lua with tabs, as the typed script it came from was.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tabs: bool,
}

/// The node-graph view of a block script (see `docs/scripting/graphs.md`).
/// The blocks are the script; this is only where the nodes are, which view
/// is shown, and value nodes wired to nothing, so it never changes the Lua.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    /// The script is shown as a node graph rather than as blocks.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shown: bool,
    /// Where nodes have been placed, in graph units, by node key: a block's
    /// id (`"12"`), or a value node's owner id and slots (`"12.0.1"`).
    /// Nodes without one are laid out after the node they're wired to.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub nodes: BTreeMap<String, [f32; 2]>,
    /// Value nodes wired to nothing. They don't run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loose: Vec<LooseValue>,
}

impl Graph {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A value node wired to nothing, with its own id (from the blocks' ids).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LooseValue {
    pub id: u64,
    pub value: Expression,
}

impl LooseValue {
    pub fn new(value: Expression) -> Self {
        Self {
            id: NEXT_BLOCK_ID.fetch_add(1, Ordering::Relaxed),
            value,
        }
    }
}

impl BlockScript {
    /// The block with `id`, wherever it is.
    pub fn find_mut(&mut self, id: u64) -> Option<&mut Block> {
        fn find(blocks: &mut [Block], id: u64) -> Option<&mut Block> {
            for block in blocks {
                if block.id == id {
                    return Some(block);
                }
                for body in block.kind.bodies_mut() {
                    if let Some(found) = find(body, id) {
                        return Some(found);
                    }
                }
            }
            None
        }
        find(&mut self.blocks, id)
    }

    /// The stack holding the block with `id`, and where in it the block is.
    pub fn stack_of_mut(&mut self, id: u64) -> Option<(&mut Vec<Block>, usize)> {
        fn find(blocks: &mut Vec<Block>, id: u64) -> Option<(&mut Vec<Block>, usize)> {
            if let Some(index) = blocks.iter().position(|block| block.id == id) {
                return Some((blocks, index));
            }
            for block in blocks.iter_mut() {
                for body in block.kind.bodies_mut() {
                    if let Some(found) = find(body, id) {
                        return Some(found);
                    }
                }
            }
            None
        }
        find(&mut self.blocks, id)
    }

    /// Calls `visit` for every block in the script, stacks and bodies alike,
    /// parents before their bodies.
    pub fn for_each_block(&self, visit: &mut impl FnMut(&Block)) {
        fn walk(blocks: &[Block], visit: &mut impl FnMut(&Block)) {
            for block in blocks {
                visit(block);
                for body in block.kind.bodies() {
                    walk(body, visit);
                }
            }
        }
        walk(&self.blocks, visit);
    }
}

/// Makes sure blocks made from now on get IDs that `script` doesn't use.
pub fn reserve_block_ids(script: &BlockScript) {
    let mut maximum = 0;
    script.for_each_block(&mut |block| maximum = maximum.max(block.id));
    for loose in &script.graph.loose {
        maximum = maximum.max(loose.id);
    }
    NEXT_BLOCK_ID.fetch_max(maximum.saturating_add(1), Ordering::Relaxed);
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub id: u64,
    pub kind: BlockKind,
    /// Where a stack's top is on the editor's canvas, for top-level blocks.
    /// `None` lays it out below the stack before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<[f32; 2]>,
    /// A blank line goes before this block's Lua, as in the typed script.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub spaced: bool,
    /// Written joined to what's before it: an if as the `elseif` of the else
    /// it's alone in, or a comment at the end of the last line.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub chained: bool,
}

impl Block {
    pub fn new(kind: BlockKind) -> Self {
        Self {
            id: NEXT_BLOCK_ID.fetch_add(1, Ordering::Relaxed),
            kind,
            position: None,
            spaced: false,
            chained: false,
        }
    }

    /// A copy with new IDs, for duplicating blocks.
    pub fn duplicate(&self) -> Self {
        let mut copy = self.clone();
        copy.renumber();
        copy
    }

    fn renumber(&mut self) {
        self.id = NEXT_BLOCK_ID.fetch_add(1, Ordering::Relaxed);
        for body in self.kind.bodies_mut() {
            for block in body {
                block.renumber();
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BlockKind {
    // Events: the top of a stack.
    WhenStarted {
        body: Vec<Block>,
    },
    WhenTouched {
        part: String,
        body: Vec<Block>,
    },
    /// World streaming brought `target` (or a Model's contents) back
    /// (Phase 80).
    WhenStreamedIn {
        target: String,
        body: Vec<Block>,
    },
    /// Client only.
    WhenKeyPressed {
        key: String,
        body: Vec<Block>,
    },
    /// When a TextButton is clicked or tapped. Client only.
    WhenClicked {
        button: String,
        body: Vec<Block>,
    },
    /// When the pointer comes onto a UI element (`MouseEnter`). Client only.
    WhenMouseEnters {
        element: String,
        body: Vec<Block>,
    },
    /// When the pointer goes off a UI element (`MouseLeave`). Client only.
    WhenMouseLeaves {
        element: String,
        body: Vec<Block>,
    },
    /// When a Slider, Toggle, Dropdown or ProgressBar's `Value` changes
    /// (`ValueChanged`): dragged, clicked, picked or set by a script. The
    /// body reads the new value as the widget's `Value` property.
    WhenValueChanged {
        widget: String,
        body: Vec<Block>,
    },
    /// Runs every frame (`RunService.Heartbeat`).
    EveryFrame {
        body: Vec<Block>,
    },
    /// Runs every frame before physics (`RunService.Stepped`).
    BeforePhysics {
        body: Vec<Block>,
    },
    /// Runs every frame just before it's drawn (`RunService.RenderStepped`).
    /// Client only.
    BeforeRender {
        body: Vec<Block>,
    },
    /// When a BindableEvent fires (`Event`): `event` is its path in
    /// ReplicatedStorage.
    WhenEventFired {
        event: String,
        body: Vec<Block>,
    },
    WhenSoundEnded {
        sound: String,
        body: Vec<Block>,
    },
    /// When a Sequence reaches its end, or is skipped there (`Completed`).
    WhenCutsceneFinished {
        sequence: String,
        body: Vec<Block>,
    },
    /// When hosting or joining fails, a join is refused or the connection
    /// is lost (`NetworkService.ConnectionFailed`, Phase 63).
    WhenConnectionFails {
        body: Vec<Block>,
    },
    /// When an Animator moves to another state (`StateChanged`).
    WhenStateChanged {
        animator: String,
        body: Vec<Block>,
    },
    /// When an Animator's clip passes the event named `event`
    /// (`AnimationEvent`).
    WhenAnimationEvent {
        animator: String,
        event: String,
        body: Vec<Block>,
    },
    /// When a NavigationAgent starts seeing or hearing the player character
    /// (`PlayerSensed`), `sense` being one of [`SENSES`].
    WhenAgentSenses {
        agent: String,
        sense: String,
        body: Vec<Block>,
    },
    /// When a SplineFollower gets to an end of its spline (`Reached`),
    /// `which` being one of [`SPLINE_ENDS`] (Phase 73).
    WhenSplineReached {
        follower: String,
        which: String,
        body: Vec<Block>,
    },
    /// When a character sits on a Seat: its `Occupant` changes to a
    /// controller (not to nil).
    WhenSeatOccupied {
        seat: String,
        body: Vec<Block>,
    },
    /// When one property of an object changes
    /// (`GetPropertyChangedSignal`), whoever changed it.
    WhenPropertyChanged {
        target: String,
        property: String,
        body: Vec<Block>,
    },
    /// When one attribute of an object changes, is added or removed
    /// (`GetAttributeChangedSignal`).
    WhenAttributeChanged {
        target: String,
        attribute: String,
        body: Vec<Block>,
    },
    /// When a ProximityPrompt is triggered: its action pressed, or held
    /// for its `HoldDuration` (Phase 44).
    WhenPromptTriggered {
        prompt: String,
        body: Vec<Block>,
    },
    /// When a ClickDetector's part is clicked (`MouseClick`).
    WhenPartClicked {
        detector: String,
        body: Vec<Block>,
    },
    /// When `command` is typed in the Developer Console (Phase 71):
    /// `ConsoleService:RegisterCommand(command, help, function() ... end)`.
    /// A LocalScript's runs on that device, a Script's on the server.
    WhenCommandRun {
        command: Expression,
        help: Expression,
        body: Vec<Block>,
    },
    /// Loose blocks on the canvas, under no event. They don't run.
    Unattached {
        body: Vec<Block>,
    },
    /// Lua at the top of the script that has no blocks, kept word for
    /// word from a script switched from Code to Blocks. It runs where it is.
    TopCode {
        code: String,
    },

    // Control.
    Wait {
        seconds: Expression,
    },
    WaitUntil {
        condition: Expression,
    },
    /// Waits until an object's event next fires (`signal:Wait()`), such as
    /// a part's `Touched`.
    WaitForEvent {
        target: String,
        event: String,
    },
    Repeat {
        count: Expression,
        body: Vec<Block>,
    },
    Forever {
        body: Vec<Block>,
    },
    RepeatUntil {
        condition: Expression,
        body: Vec<Block>,
    },
    If {
        condition: Expression,
        body: Vec<Block>,
    },
    IfElse {
        condition: Expression,
        then_body: Vec<Block>,
        else_body: Vec<Block>,
    },
    /// Ends the event's handler early.
    StopScript,
    /// Runs `body` once for each child of `target` (the Workspace when
    /// empty), with the child in the variable `variable`.
    ForEachChild {
        target: String,
        variable: String,
        body: Vec<Block>,
    },
    /// Runs `body` once for each part whose bounding box is within `radius`
    /// studs of `center` (`workspace:GetPartBoundsInRadius`), with the part
    /// in the variable `variable`.
    ForEachPartInRadius {
        center: Expression,
        radius: Expression,
        variable: String,
        body: Vec<Block>,
    },
    /// Runs `body` once for each part overlapping `target`, such as the
    /// parts inside a trigger zone (`workspace:GetPartsInPart`), with the
    /// part in the variable `variable`.
    ForEachPartTouching {
        target: String,
        variable: String,
        body: Vec<Block>,
    },
    /// Waits until `target` (the Workspace when empty) has a child `name`.
    WaitForChild {
        target: String,
        name: String,
    },
    /// Destroys `target` after `seconds`, without waiting
    /// (`Debris:AddItem`).
    DestroyAfter {
        target: String,
        seconds: Expression,
    },

    // Motion.
    MoveTo {
        target: String,
        position: Expression,
    },
    MoveBy {
        target: String,
        offset: Expression,
    },
    /// Turns `target` to face `position`: `CFrame.lookAt`.
    LookAt {
        target: String,
        position: Expression,
    },
    /// Moves `target` along the way it faces (its `CFrame.LookVector`).
    MoveForward {
        target: String,
        distance: Expression,
    },
    TurnTo {
        target: String,
        orientation: Expression,
    },
    TurnBy {
        target: String,
        rotation: Expression,
    },
    SetAnchored {
        target: String,
        anchored: Expression,
    },
    /// Pushes `target` with an impulse: `ApplyImpulse`, which changes its
    /// velocity by `force` over its mass.
    PushWithForce {
        target: String,
        force: Expression,
    },
    /// Throws `target`: sets its `AssemblyLinearVelocity`.
    SetPartVelocity {
        target: String,
        velocity: Expression,
    },
    /// Sets `target`'s friction, keeping its other physical properties:
    /// its `CustomPhysicalProperties` from its `CurrentPhysicalProperties`.
    SetFriction {
        target: String,
        friction: Expression,
    },

    // Looks.
    SetColor {
        target: String,
        color: Expression,
    },
    SetText {
        target: String,
        text: Expression,
    },
    Show {
        target: String,
    },
    Hide {
        target: String,
    },
    /// Sets a PageLayout's current page to a Frame's name.
    ShowPage {
        layout: String,
        page: Expression,
    },
    /// Scrolls a ScrollingFrame: sets its `CanvasPosition` to `x`, `y` pixels.
    ScrollTo {
        target: String,
        x: Expression,
        y: Expression,
    },
    /// Sets a Slider, Toggle, Dropdown or ProgressBar's `Value`, which
    /// moves it and fires its `ValueChanged`.
    SetValue {
        target: String,
        value: Expression,
    },
    SetProperty {
        target: String,
        property: String,
        value: Expression,
    },
    /// Sets one of an object's attributes (`SetAttribute`); nil removes it.
    SetAttribute {
        target: String,
        attribute: String,
        value: Expression,
    },

    // Navigation.
    /// Walks a NavigationAgent along the navigation mesh to `position`: sets
    /// its `Destination`.
    SetDestination {
        agent: String,
        position: Expression,
    },
    /// Stands a character controller's character up from its seat: sets
    /// its `Sit` to false.
    LeaveSeat {
        controller: String,
    },
    /// Moves a NavigationAgent straight at `velocity` studs a second, with
    /// no path: sets its `Velocity`. Zero stops it where it is.
    SetVelocity {
        agent: String,
        velocity: Expression,
    },

    // Saving: `SaveService`.
    /// Writes save slot `slot`: player data, world state, the active
    /// checkpoint and settings.
    SaveSlot {
        slot: Expression,
    },
    /// Puts the game back as save slot `slot` was saved.
    LoadSlot {
        slot: Expression,
    },
    /// Makes a Checkpoint the active one, capturing the game there.
    SetCheckpoint {
        checkpoint: String,
    },
    /// Returns to the active checkpoint: what was captured there, with the
    /// player character standing on it.
    RestoreCheckpoint,

    // Levels: `LevelService` (Phase 48).
    /// Puts the level `level` in place of the Workspace, with characters
    /// at the Instance called `spawn` there (empty text: a SpawnLocation).
    LoadLevel {
        level: Expression,
        spawn: Expression,
    },
    /// Copies the level `level` into a Model beside the current level.
    AddLevel {
        level: Expression,
    },
    /// Takes the added level `level` out again.
    RemoveLevel {
        level: Expression,
    },

    // Multiplayer: `NetworkService` (Phase 63).
    /// Hosts the game on UDP port `port`, for other players to join.
    HostGame {
        port: Expression,
    },
    /// Joins the game at `address` (`"192.168.1.20:47800"`).
    JoinGame {
        address: Expression,
    },
    /// Leaves the game joined, or stops hosting.
    LeaveGame,
    /// Gives `target`'s assembly to `owner` (a Player, or nil for the
    /// server) to simulate: `SetNetworkOwner` (Phase 66).
    SetNetworkOwner {
        target: String,
        owner: Expression,
    },
    /// Lets the engine choose who simulates `target`'s assembly: the
    /// nearest player: `SetNetworkOwnershipAuto` (Phase 66).
    SetNetworkOwnershipAuto {
        target: String,
    },
    /// Takes `player` out of the game, telling them `reason`:
    /// `Player:Kick` (Phase 67).
    KickPlayer {
        player: Expression,
        reason: Expression,
    },
    /// Loads what's around `position` for `player` and waits for it:
    /// `Player:RequestStreamAroundAsync` (Phase 80).
    RequestStreamAround {
        player: Expression,
        position: Expression,
    },
    /// Sends `text` to the chat as this device's player:
    /// `TextChatService:SendMessage` (LocalScripts, Phase 67).
    SendChatMessage {
        text: Expression,
    },
    /// Shows `text` in the chat as the game's:
    /// `TextChatService:DisplaySystemMessage` (Phase 67).
    ShowSystemMessage {
        text: Expression,
    },

    // Camera: `workspace.CurrentCamera` (LocalScripts only).
    /// Sets `CameraType` to one of [`CAMERA_TYPES`]: Scriptable leaves the
    /// camera to scripts, Custom gives it back to the controller.
    SetCameraType {
        camera_type: String,
    },
    /// Turns the camera to look at `position`, from where it is.
    PointCameraAt {
        position: Expression,
    },
    /// `Camera:Shake(intensity, seconds)`.
    ShakeCamera {
        intensity: Expression,
        seconds: Expression,
    },
    /// Places a decal that fades out after `seconds`
    /// (`workspace:SpawnDecal`): the picture `texture` laid on what's at
    /// `position`, facing `facing` (a surface's normal), `size` studs.
    SpawnDecal {
        texture: Expression,
        position: Expression,
        facing: Expression,
        size: Expression,
        seconds: Expression,
    },

    // Localization: `LocalizationService`.
    /// Sets the language bound UI text shows (`LocalizationService.Language`).
    SetLanguage {
        language: Expression,
    },

    // Lighting: the world's look.
    /// Sets `Lighting.ToneMapping` to one of [`TONE_MAPPINGS`].
    SetToneMapping {
        curve: String,
    },
    /// Sets `Lighting.ClockTime`: the time of day, in hours.
    SetClockTime {
        time: Expression,
    },
    /// Sets one of [`ATMOSPHERE_NUMBERS`] on Lighting's Atmosphere (the
    /// child named Atmosphere).
    SetAtmosphere {
        property: String,
        value: Expression,
    },

    // Graphics: the player's computer's settings (LocalScripts only).
    /// Sets `UserGameSettings.GraphicsQuality` to one of
    /// [`GRAPHICS_QUALITIES`].
    SetGraphicsQuality {
        quality: String,
    },
    /// Sets one of [`GRAPHICS_OPTIONS`] in `UserGameSettings` to one of its
    /// [`graphics_choices`]; [`PRESET`] goes back to the preset's.
    SetGraphicsOption {
        setting: String,
        choice: String,
    },
    /// Sets one of [`GRAPHICS_NUMBERS`] in `UserGameSettings`.
    SetGraphicsNumber {
        setting: String,
        value: Expression,
    },

    // Sound.
    PlaySound {
        target: String,
    },
    StopSound {
        target: String,
    },
    PauseSound {
        target: String,
    },
    ResumeSound {
        target: String,
    },
    SetVolume {
        target: String,
        volume: Expression,
    },

    // Animation: an Animator's parameters and states.
    SetAnimatorBool {
        animator: String,
        parameter: String,
        value: Expression,
    },
    SetAnimatorNumber {
        animator: String,
        parameter: String,
        value: Expression,
    },
    SetAnimatorTrigger {
        animator: String,
        parameter: String,
    },
    ResetAnimatorTrigger {
        animator: String,
        parameter: String,
    },
    PlayAnimatorState {
        animator: String,
        state: String,
    },
    /// How much a state machine layer counts, faded over `seconds`
    /// (`animator:SetLayerWeight(layer, weight, seconds)`, Phase 78).
    SetAnimatorLayerWeight {
        animator: String,
        layer: String,
        weight: Expression,
        seconds: Expression,
    },
    /// Loads an Animation on an Animator and plays the track
    /// (`animator:LoadAnimation(animation):Play()`).
    PlayAnimation {
        animator: String,
        animation: String,
    },
    /// Plays a Sequence (`sequence:Play()`, Phase 51).
    PlayCutscene {
        sequence: String,
    },
    /// Puts an object `distance` studs along a Spline, facing along it
    /// (`target:PivotTo(spline:GetCFrameAt(distance))`, Phase 73).
    PlaceOnSpline {
        target: String,
        spline: String,
        distance: Expression,
    },
    /// Has a NavigationAgent walk a Spline's points in order
    /// (`agent:Patrol(spline)`).
    Patrol {
        agent: String,
        spline: String,
    },

    // Tweens: a property moving smoothly (`TweenService`).
    /// Moves one property of an object to `value` over `seconds`, eased by
    /// `style` (one of [`EASING_STYLES`]) and `direction` (one of
    /// [`EASING_DIRECTIONS`]).
    Tween {
        target: String,
        property: String,
        value: Expression,
        seconds: Expression,
        style: String,
        direction: String,
    },
    /// Slides a UI element in to where it is from one parent's width or
    /// height past an edge, `from` being one of [`SLIDE_EDGES`].
    SlideIn {
        target: String,
        from: String,
        seconds: Expression,
    },
    /// Fades a UI element's background in (`BackgroundTransparency` to 0).
    FadeIn {
        target: String,
        seconds: Expression,
    },
    /// Fades a UI element's background out (`BackgroundTransparency` to 1).
    FadeOut {
        target: String,
        seconds: Expression,
    },

    // Variables.
    SetVariable {
        name: String,
        value: Expression,
    },
    ChangeVariable {
        name: String,
        by: Expression,
    },

    // Other.
    Print {
        value: Expression,
    },
    Destroy {
        target: String,
    },
    /// Copies `target` and puts the copy in `parent` (the Workspace when empty).
    CloneInto {
        target: String,
        parent: String,
    },
    /// Fires a BindableEvent (`event:Fire()`), by its path in ReplicatedStorage.
    FireEvent {
        event: String,
    },

    // Luau itself, which any typed script is made of (see
    // `docs/scripting/blocks.md`). Names, parameters and types are text, as
    // written, so `a: number, ...` is one parameter list.
    /// The statements at the script's top level, in order: not an event,
    /// and written without anything around them.
    RunScript {
        body: Vec<Block>,
    },
    /// `signal:Connect(function(params) ... end)` at the top level.
    OnSignal {
        signal: Expression,
        method: String,
        params: String,
        body: Vec<Block>,
    },
    /// `signal:Connect(function(params) ... end)` among other blocks.
    Connect {
        signal: Expression,
        method: String,
        params: String,
        body: Vec<Block>,
    },
    /// One or more `--` comments, as written.
    Comment {
        text: String,
    },
    /// `local names = values`.
    Local {
        names: String,
        values: Vec<Expression>,
    },
    /// `targets = values`.
    Assign {
        targets: Vec<Expression>,
        values: Vec<Expression>,
    },
    /// `target += value`; `op` is the operator without the `=`.
    CompoundAssign {
        target: Expression,
        op: String,
        value: Expression,
    },
    /// `function(args)` as a statement.
    CallFunction {
        function: Expression,
        args: Vec<Expression>,
    },
    /// `object:method(args)` as a statement.
    CallMethod {
        object: Expression,
        method: String,
        args: Vec<Expression>,
    },
    /// `local function name(params) ... end`, or `function name(params) ...
    /// end` with a name such as `a.b:c`.
    DefineFunction {
        name: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        local: bool,
        params: String,
        /// `<T>`, as written.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        generics: String,
        /// `: number`, as written.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        returns: String,
        /// `@native`, as written.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        attributes: String,
        body: Vec<Block>,
    },
    /// `while condition do ... end`.
    While {
        condition: Expression,
        body: Vec<Block>,
    },
    /// `repeat ... until condition`.
    RepeatLoop {
        body: Vec<Block>,
        condition: Expression,
    },
    /// `for variable = start, stop, step do ... end`.
    NumericFor {
        variable: String,
        start: Expression,
        stop: Expression,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<Expression>,
        body: Vec<Block>,
    },
    /// `for names in values do ... end`.
    GenericFor {
        names: String,
        values: Vec<Expression>,
        body: Vec<Block>,
    },
    /// `do ... end`.
    DoBlock {
        body: Vec<Block>,
    },
    Break,
    Continue,
    /// `return values`.
    Return {
        values: Vec<Expression>,
    },

    /// Lua that has no blocks, kept word for word from a script switched
    /// from Code to Blocks.
    Code {
        code: String,
    },
}

/// The binary operators Luau has, which [`Expression::Binary`] can be.
pub const LUA_BINARY_OPS: [&str; 16] = [
    "+", "-", "*", "/", "//", "%", "^", "..", "==", "~=", "<", "<=", ">", ">=", "and", "or",
];
/// Luau's unary operators.
pub const LUA_UNARY_OPS: [&str; 3] = ["not", "-", "#"];
/// The operators a compound assignment can use.
pub const LUA_COMPOUND_OPS: [&str; 8] = ["+", "-", "*", "/", "//", "%", "^", ".."];
/// The ways to listen to a signal.
pub const SIGNAL_METHODS: [&str; 3] = ["Connect", "Once", "ConnectParallel"];

/// `Enum.EasingStyle`'s items, which the tween block offers.
pub const EASING_STYLES: [&str; 7] = [
    "Linear", "Sine", "Quad", "Cubic", "Back", "Bounce", "Elastic",
];
/// `Enum.EasingDirection`'s items.
pub const EASING_DIRECTIONS: [&str; 3] = ["In", "Out", "InOut"];
/// The edges "slide in" can come from.
pub const SLIDE_EDGES: [&str; 4] = ["Left", "Right", "Top", "Bottom"];
/// `Enum.CameraType`'s items, which the camera type block offers.
pub const CAMERA_TYPES: [&str; 5] = ["Scriptable", "Custom", "Follow", "Track", "Fixed"];
/// `Enum.ToneMapping`'s items, which the tone mapping block offers.
pub const TONE_MAPPINGS: [&str; 3] = ["Classic", "Neutral", "Filmic"];
/// The Atmosphere numbers the atmosphere block sets.
pub const ATMOSPHERE_NUMBERS: [&str; 4] = ["Density", "Offset", "Glare", "Haze"];
/// `Enum.GraphicsQuality`'s items, which the graphics quality block offers.
pub const GRAPHICS_QUALITIES: [&str; 5] = ["Automatic", "Low", "Medium", "High", "Ultra"];
/// The `UserGameSettings` overrides the graphics option block sets.
pub const GRAPHICS_OPTIONS: [&str; 20] = [
    "ShadowQuality",
    "AntiAliasing",
    "TextureQuality",
    "AmbientOcclusion",
    "ReflectionQuality",
    "CloudQuality",
    "DepthOfField",
    "MotionBlur",
    "RayTracedShadows",
    "RayTracedAmbientOcclusion",
    "RayTracedGlobalIllumination",
    "PostProcessing",
    "ContactShadows",
    "RealtimeGI",
    "VolumetricFog",
    "Vignette",
    "FilmGrain",
    "ChromaticAberration",
    "Sharpen",
    "SunRays",
];
/// The [`GRAPHICS_OPTIONS`] that are on or off (booleans in Lua), not enums.
pub const GRAPHICS_SWITCHES: [&str; 9] = [
    "PostProcessing",
    "ContactShadows",
    "RealtimeGI",
    "VolumetricFog",
    "Vignette",
    "FilmGrain",
    "ChromaticAberration",
    "Sharpen",
    "SunRays",
];
/// The `UserGameSettings` overrides the graphics number block sets.
pub const GRAPHICS_NUMBERS: [&str; 2] = ["ShadowDistance", "RenderScale"];
/// The graphics option block's choice that removes the override (nil).
pub const PRESET: &str = "Preset";

/// What the graphics option block can set `setting` (one of
/// [`GRAPHICS_OPTIONS`]) to: its enum's items (On and Off for the
/// [`GRAPHICS_SWITCHES`]), then [`PRESET`]. Empty for any other name.
pub fn graphics_choices(setting: &str) -> &'static [&'static str] {
    match setting {
        "ShadowQuality" => &["Off", "Low", "Medium", "High", "Ultra", PRESET],
        "AntiAliasing" => &["Off", "FXAA", "MSAA2", "MSAA4", "TAA", PRESET],
        "TextureQuality" => &["Quarter", "Half", "Full", PRESET],
        "AmbientOcclusion" | "DepthOfField" | "MotionBlur" => &["Off", "Low", "High", PRESET],
        "ReflectionQuality" | "CloudQuality" => &["Low", "Medium", "High", "Ultra", PRESET],
        "RayTracedShadows" | "RayTracedAmbientOcclusion" | "RayTracedGlobalIllumination" => {
            &["Off", "Low", "Medium", "High", PRESET]
        }
        "PostProcessing"
        | "ContactShadows"
        | "RealtimeGI"
        | "VolumetricFog"
        | "Vignette"
        | "FilmGrain"
        | "ChromaticAberration"
        | "Sharpen"
        | "SunRays" => &["On", "Off", PRESET],
        _ => &[],
    }
}

/// The classes the "is a" block offers, abstract ones first.
pub const IS_A_CLASSES: [&str; 16] = [
    "BasePart",
    "GuiObject",
    "Part",
    "MeshPart",
    "Model",
    "Folder",
    "Frame",
    "TextLabel",
    "TextButton",
    "ImageLabel",
    "Sound",
    "Light",
    "ParticleEmitter",
    "Script",
    "LocalScript",
    "ModuleScript",
];

/// The senses a NavigationAgent's `PlayerSensed` names.
pub const SENSES: [&str; 2] = ["Sight", "Hearing"];

/// The ends a SplineFollower's `Reached` names.
pub const SPLINE_ENDS: [&str; 2] = ["End", "Start"];

impl BlockKind {
    pub fn shape(&self) -> BlockShape {
        if self.is_event() {
            BlockShape::Event
        } else {
            BlockShape::Statement
        }
    }

    /// Event blocks, and unattached stacks: what can be at the top level.
    pub fn is_event(&self) -> bool {
        matches!(
            self,
            BlockKind::WhenStarted { .. }
                | BlockKind::WhenTouched { .. }
                | BlockKind::WhenStreamedIn { .. }
                | BlockKind::WhenKeyPressed { .. }
                | BlockKind::WhenClicked { .. }
                | BlockKind::WhenMouseEnters { .. }
                | BlockKind::WhenMouseLeaves { .. }
                | BlockKind::WhenValueChanged { .. }
                | BlockKind::EveryFrame { .. }
                | BlockKind::BeforePhysics { .. }
                | BlockKind::BeforeRender { .. }
                | BlockKind::WhenEventFired { .. }
                | BlockKind::WhenSoundEnded { .. }
                | BlockKind::WhenCutsceneFinished { .. }
                | BlockKind::WhenConnectionFails { .. }
                | BlockKind::WhenStateChanged { .. }
                | BlockKind::WhenAnimationEvent { .. }
                | BlockKind::WhenAgentSenses { .. }
                | BlockKind::WhenSplineReached { .. }
                | BlockKind::WhenSeatOccupied { .. }
                | BlockKind::WhenPropertyChanged { .. }
                | BlockKind::WhenAttributeChanged { .. }
                | BlockKind::WhenPromptTriggered { .. }
                | BlockKind::WhenPartClicked { .. }
                | BlockKind::WhenCommandRun { .. }
                | BlockKind::Unattached { .. }
                | BlockKind::TopCode { .. }
                | BlockKind::RunScript { .. }
                | BlockKind::OnSignal { .. }
        )
    }

    /// Whether this block only works in client block scripts (LocalScripts).
    pub fn client_only(&self) -> bool {
        matches!(
            self,
            BlockKind::WhenKeyPressed { .. }
                | BlockKind::WhenClicked { .. }
                | BlockKind::WhenMouseEnters { .. }
                | BlockKind::WhenMouseLeaves { .. }
                | BlockKind::BeforeRender { .. }
                | BlockKind::SetCameraType { .. }
                | BlockKind::PointCameraAt { .. }
                | BlockKind::ShakeCamera { .. }
        )
    }

    /// The stacks of blocks inside this one, in order (an if/else's two),
    /// then the functions written in its value slots: those directly in a
    /// slot first, as they're the ones drawn as mouths on a block.
    pub fn bodies(&self) -> Vec<&Vec<Block>> {
        let mut bodies = match self {
            BlockKind::WhenStarted { body }
            | BlockKind::WhenTouched { body, .. }
            | BlockKind::WhenStreamedIn { body, .. }
            | BlockKind::WhenKeyPressed { body, .. }
            | BlockKind::WhenClicked { body, .. }
            | BlockKind::WhenMouseEnters { body, .. }
            | BlockKind::WhenMouseLeaves { body, .. }
            | BlockKind::WhenValueChanged { body, .. }
            | BlockKind::EveryFrame { body }
            | BlockKind::BeforePhysics { body }
            | BlockKind::BeforeRender { body }
            | BlockKind::WhenEventFired { body, .. }
            | BlockKind::WhenSoundEnded { body, .. }
            | BlockKind::WhenCutsceneFinished { body, .. }
            | BlockKind::WhenConnectionFails { body }
            | BlockKind::WhenStateChanged { body, .. }
            | BlockKind::WhenAnimationEvent { body, .. }
            | BlockKind::WhenAgentSenses { body, .. }
            | BlockKind::WhenSplineReached { body, .. }
            | BlockKind::WhenSeatOccupied { body, .. }
            | BlockKind::WhenPropertyChanged { body, .. }
            | BlockKind::WhenAttributeChanged { body, .. }
            | BlockKind::WhenPromptTriggered { body, .. }
            | BlockKind::WhenPartClicked { body, .. }
            | BlockKind::WhenCommandRun { body, .. }
            | BlockKind::Unattached { body }
            | BlockKind::Repeat { body, .. }
            | BlockKind::Forever { body }
            | BlockKind::RepeatUntil { body, .. }
            | BlockKind::ForEachChild { body, .. }
            | BlockKind::ForEachPartInRadius { body, .. }
            | BlockKind::ForEachPartTouching { body, .. }
            | BlockKind::If { body, .. }
            | BlockKind::RunScript { body }
            | BlockKind::OnSignal { body, .. }
            | BlockKind::Connect { body, .. }
            | BlockKind::DefineFunction { body, .. }
            | BlockKind::While { body, .. }
            | BlockKind::RepeatLoop { body, .. }
            | BlockKind::NumericFor { body, .. }
            | BlockKind::GenericFor { body, .. }
            | BlockKind::DoBlock { body } => vec![body],
            BlockKind::IfElse {
                then_body,
                else_body,
                ..
            } => vec![then_body, else_body],
            _ => Vec::new(),
        };
        let slots = self.expressions();
        bodies.extend(slots.iter().filter_map(|slot| match slot {
            Expression::Closure { body, .. } => Some(body),
            _ => None,
        }));
        for slot in slots {
            if !matches!(slot, Expression::Closure { .. }) {
                slot.closure_bodies(&mut bodies);
            }
        }
        bodies
    }

    /// Like [`BlockKind::bodies`], for changing them.
    pub fn bodies_mut(&mut self) -> Vec<&mut Vec<Block>> {
        // The slots of the kinds that also have stacks, which are separate
        // fields of the variant, so both can be borrowed at once.
        let (mut bodies, slots): (Vec<&mut Vec<Block>>, Vec<&mut Expression>) = match self {
            BlockKind::Repeat { count: e, body }
            | BlockKind::RepeatUntil { condition: e, body }
            | BlockKind::If { condition: e, body }
            | BlockKind::OnSignal {
                signal: e, body, ..
            }
            | BlockKind::Connect {
                signal: e, body, ..
            }
            | BlockKind::While { condition: e, body }
            | BlockKind::RepeatLoop { condition: e, body } => (vec![body], vec![e]),
            BlockKind::IfElse {
                condition,
                then_body,
                else_body,
            } => (vec![then_body, else_body], vec![condition]),
            BlockKind::ForEachPartInRadius {
                center,
                radius,
                body,
                ..
            } => (vec![body], vec![center, radius]),
            BlockKind::WhenCommandRun {
                command,
                help,
                body,
            } => (vec![body], vec![command, help]),
            BlockKind::NumericFor {
                start,
                stop,
                step,
                body,
                ..
            } => {
                let mut slots = vec![start, stop];
                slots.extend(step.as_mut());
                (vec![body], slots)
            }
            BlockKind::GenericFor { values, body, .. } => (vec![body], values.iter_mut().collect()),
            BlockKind::WhenStarted { body }
            | BlockKind::WhenStreamedIn { body, .. }
            | BlockKind::WhenTouched { body, .. }
            | BlockKind::WhenKeyPressed { body, .. }
            | BlockKind::WhenClicked { body, .. }
            | BlockKind::WhenMouseEnters { body, .. }
            | BlockKind::WhenMouseLeaves { body, .. }
            | BlockKind::WhenValueChanged { body, .. }
            | BlockKind::EveryFrame { body }
            | BlockKind::BeforePhysics { body }
            | BlockKind::BeforeRender { body }
            | BlockKind::WhenEventFired { body, .. }
            | BlockKind::WhenSoundEnded { body, .. }
            | BlockKind::WhenCutsceneFinished { body, .. }
            | BlockKind::WhenConnectionFails { body }
            | BlockKind::WhenStateChanged { body, .. }
            | BlockKind::WhenAnimationEvent { body, .. }
            | BlockKind::WhenAgentSenses { body, .. }
            | BlockKind::WhenSplineReached { body, .. }
            | BlockKind::WhenSeatOccupied { body, .. }
            | BlockKind::WhenPropertyChanged { body, .. }
            | BlockKind::WhenAttributeChanged { body, .. }
            | BlockKind::WhenPromptTriggered { body, .. }
            | BlockKind::WhenPartClicked { body, .. }
            | BlockKind::Unattached { body }
            | BlockKind::Forever { body }
            | BlockKind::ForEachChild { body, .. }
            | BlockKind::ForEachPartTouching { body, .. }
            | BlockKind::RunScript { body }
            | BlockKind::DefineFunction { body, .. }
            | BlockKind::DoBlock { body } => (vec![body], Vec::new()),
            other => (Vec::new(), other.expressions_mut()),
        };
        let (direct, nested): (Vec<_>, Vec<_>) = slots
            .into_iter()
            .partition(|slot| matches!(slot, Expression::Closure { .. }));
        for slot in direct {
            if let Expression::Closure { body, .. } = slot {
                bodies.push(body);
            }
        }
        for slot in nested {
            slot.closure_bodies_mut(&mut bodies);
        }
        bodies
    }

    /// Like [`BlockKind::expressions`], for changing them.
    pub fn expressions_mut(&mut self) -> Vec<&mut Expression> {
        match self {
            BlockKind::Wait { seconds: e }
            | BlockKind::WaitUntil { condition: e }
            | BlockKind::Repeat { count: e, .. }
            | BlockKind::RepeatUntil { condition: e, .. }
            | BlockKind::If { condition: e, .. }
            | BlockKind::IfElse { condition: e, .. }
            | BlockKind::MoveTo { position: e, .. }
            | BlockKind::MoveBy { offset: e, .. }
            | BlockKind::LookAt { position: e, .. }
            | BlockKind::MoveForward { distance: e, .. }
            | BlockKind::TurnTo { orientation: e, .. }
            | BlockKind::TurnBy { rotation: e, .. }
            | BlockKind::SetAnchored { anchored: e, .. }
            | BlockKind::PushWithForce { force: e, .. }
            | BlockKind::SetPartVelocity { velocity: e, .. }
            | BlockKind::SetFriction { friction: e, .. }
            | BlockKind::SetColor { color: e, .. }
            | BlockKind::SetText { text: e, .. }
            | BlockKind::ShowPage { page: e, .. }
            | BlockKind::SetProperty { value: e, .. }
            | BlockKind::SetAttribute { value: e, .. }
            | BlockKind::SetValue { value: e, .. }
            | BlockKind::SetDestination { position: e, .. }
            | BlockKind::SetVelocity { velocity: e, .. }
            | BlockKind::SaveSlot { slot: e }
            | BlockKind::LoadSlot { slot: e }
            | BlockKind::AddLevel { level: e }
            | BlockKind::RemoveLevel { level: e }
            | BlockKind::HostGame { port: e }
            | BlockKind::JoinGame { address: e }
            | BlockKind::SetNetworkOwner { owner: e, .. }
            | BlockKind::SendChatMessage { text: e }
            | BlockKind::ShowSystemMessage { text: e }
            | BlockKind::SetLanguage { language: e }
            | BlockKind::SetVolume { volume: e, .. }
            | BlockKind::PlaceOnSpline { distance: e, .. }
            | BlockKind::SetAnimatorBool { value: e, .. }
            | BlockKind::SetAnimatorNumber { value: e, .. }
            | BlockKind::SetVariable { value: e, .. }
            | BlockKind::ChangeVariable { by: e, .. }
            | BlockKind::SlideIn { seconds: e, .. }
            | BlockKind::FadeIn { seconds: e, .. }
            | BlockKind::FadeOut { seconds: e, .. }
            | BlockKind::SetGraphicsNumber { value: e, .. }
            | BlockKind::SetClockTime { time: e }
            | BlockKind::SetAtmosphere { value: e, .. }
            | BlockKind::PointCameraAt { position: e }
            | BlockKind::DestroyAfter { seconds: e, .. }
            | BlockKind::Print { value: e } => vec![e],
            BlockKind::Tween { value, seconds, .. } => vec![value, seconds],
            BlockKind::LoadLevel { level, spawn } => vec![level, spawn],
            BlockKind::WhenCommandRun { command, help, .. } => vec![command, help],
            BlockKind::KickPlayer { player, reason } => vec![player, reason],
            BlockKind::RequestStreamAround { player, position } => vec![player, position],
            BlockKind::ScrollTo { x, y, .. } => vec![x, y],
            BlockKind::ShakeCamera { intensity, seconds } => vec![intensity, seconds],
            BlockKind::SetAnimatorLayerWeight {
                weight, seconds, ..
            } => vec![weight, seconds],
            BlockKind::SpawnDecal {
                texture,
                position,
                facing,
                size,
                seconds,
            } => vec![texture, position, facing, size, seconds],
            BlockKind::Local { values, .. } | BlockKind::Return { values } => {
                values.iter_mut().collect()
            }
            BlockKind::GenericFor { values, .. } => values.iter_mut().collect(),
            BlockKind::Assign { targets, values } => {
                targets.iter_mut().chain(values.iter_mut()).collect()
            }
            BlockKind::CompoundAssign { target, value, .. } => vec![target, value],
            BlockKind::CallFunction {
                function: first,
                args,
            }
            | BlockKind::CallMethod {
                object: first,
                args,
                ..
            } => std::iter::once(first).chain(args.iter_mut()).collect(),
            BlockKind::OnSignal { signal: e, .. }
            | BlockKind::Connect { signal: e, .. }
            | BlockKind::While { condition: e, .. }
            | BlockKind::RepeatLoop { condition: e, .. } => vec![e],
            BlockKind::NumericFor {
                start, stop, step, ..
            } => {
                let mut slots = vec![start, stop];
                slots.extend(step.as_mut());
                slots
            }
            BlockKind::ForEachPartInRadius { center, radius, .. } => vec![center, radius],
            _ => Vec::new(),
        }
    }

    /// The world's look blocks, which need `Lighting`.
    pub fn sets_lighting(&self) -> bool {
        matches!(
            self,
            BlockKind::SetToneMapping { .. }
                | BlockKind::SetClockTime { .. }
                | BlockKind::SetAtmosphere { .. }
        )
    }

    /// Graphics settings blocks, which need `UserGameSettings`.
    pub fn sets_graphics(&self) -> bool {
        matches!(
            self,
            BlockKind::SetGraphicsQuality { .. }
                | BlockKind::SetGraphicsOption { .. }
                | BlockKind::SetGraphicsNumber { .. }
        )
    }

    /// Tween blocks, which need `TweenService`.
    pub fn tweens(&self) -> bool {
        matches!(
            self,
            BlockKind::Tween { .. }
                | BlockKind::SlideIn { .. }
                | BlockKind::FadeIn { .. }
                | BlockKind::FadeOut { .. }
        )
    }

    /// Save and checkpoint blocks, which need `SaveService`.
    pub fn saves(&self) -> bool {
        matches!(
            self,
            BlockKind::SaveSlot { .. }
                | BlockKind::LoadSlot { .. }
                | BlockKind::SetCheckpoint { .. }
                | BlockKind::RestoreCheckpoint
        )
    }

    /// Level blocks, which need `LevelService`.
    pub fn levels(&self) -> bool {
        matches!(
            self,
            BlockKind::LoadLevel { .. }
                | BlockKind::AddLevel { .. }
                | BlockKind::RemoveLevel { .. }
        )
    }

    /// Multiplayer blocks, which need `NetworkService`.
    pub fn networks(&self) -> bool {
        matches!(
            self,
            BlockKind::HostGame { .. }
                | BlockKind::JoinGame { .. }
                | BlockKind::LeaveGame
                | BlockKind::WhenConnectionFails { .. }
        )
    }

    /// Blocks nothing can go after: they never finish ("forever") or end
    /// the script ("stop this script").
    pub fn is_cap(&self) -> bool {
        matches!(
            self,
            BlockKind::Forever { .. }
                | BlockKind::StopScript
                | BlockKind::Break
                | BlockKind::Continue
                | BlockKind::Return { .. }
        )
    }

    /// The list of inputs this block can have more or fewer of, if it has
    /// one: a call's arguments, or a local's or a return's values.
    pub fn input_list_mut(&mut self) -> Option<&mut Vec<Expression>> {
        match self {
            BlockKind::CallFunction { args: list, .. }
            | BlockKind::CallMethod { args: list, .. }
            | BlockKind::Local { values: list, .. }
            | BlockKind::Assign { values: list, .. }
            | BlockKind::Return { values: list }
            | BlockKind::GenericFor { values: list, .. } => Some(list),
            _ => None,
        }
    }

    /// The value slots this block has, in the order they're shown.
    pub fn expressions(&self) -> Vec<&Expression> {
        match self {
            BlockKind::Wait { seconds: e }
            | BlockKind::WaitUntil { condition: e }
            | BlockKind::Repeat { count: e, .. }
            | BlockKind::RepeatUntil { condition: e, .. }
            | BlockKind::If { condition: e, .. }
            | BlockKind::IfElse { condition: e, .. }
            | BlockKind::MoveTo { position: e, .. }
            | BlockKind::MoveBy { offset: e, .. }
            | BlockKind::LookAt { position: e, .. }
            | BlockKind::MoveForward { distance: e, .. }
            | BlockKind::TurnTo { orientation: e, .. }
            | BlockKind::TurnBy { rotation: e, .. }
            | BlockKind::SetAnchored { anchored: e, .. }
            | BlockKind::PushWithForce { force: e, .. }
            | BlockKind::SetPartVelocity { velocity: e, .. }
            | BlockKind::SetFriction { friction: e, .. }
            | BlockKind::SetColor { color: e, .. }
            | BlockKind::SetText { text: e, .. }
            | BlockKind::ShowPage { page: e, .. }
            | BlockKind::SetProperty { value: e, .. }
            | BlockKind::SetAttribute { value: e, .. }
            | BlockKind::SetValue { value: e, .. }
            | BlockKind::SetDestination { position: e, .. }
            | BlockKind::SetVelocity { velocity: e, .. }
            | BlockKind::SaveSlot { slot: e }
            | BlockKind::LoadSlot { slot: e }
            | BlockKind::AddLevel { level: e }
            | BlockKind::RemoveLevel { level: e }
            | BlockKind::HostGame { port: e }
            | BlockKind::JoinGame { address: e }
            | BlockKind::SetNetworkOwner { owner: e, .. }
            | BlockKind::SendChatMessage { text: e }
            | BlockKind::ShowSystemMessage { text: e }
            | BlockKind::SetLanguage { language: e }
            | BlockKind::SetVolume { volume: e, .. }
            | BlockKind::PlaceOnSpline { distance: e, .. }
            | BlockKind::SetAnimatorBool { value: e, .. }
            | BlockKind::SetAnimatorNumber { value: e, .. }
            | BlockKind::SetVariable { value: e, .. }
            | BlockKind::ChangeVariable { by: e, .. }
            | BlockKind::SlideIn { seconds: e, .. }
            | BlockKind::FadeIn { seconds: e, .. }
            | BlockKind::FadeOut { seconds: e, .. }
            | BlockKind::SetGraphicsNumber { value: e, .. }
            | BlockKind::SetClockTime { time: e }
            | BlockKind::SetAtmosphere { value: e, .. }
            | BlockKind::PointCameraAt { position: e }
            | BlockKind::DestroyAfter { seconds: e, .. }
            | BlockKind::Print { value: e } => vec![e],
            BlockKind::Tween { value, seconds, .. } => vec![value, seconds],
            BlockKind::LoadLevel { level, spawn } => vec![level, spawn],
            BlockKind::WhenCommandRun { command, help, .. } => vec![command, help],
            BlockKind::RequestStreamAround { player, position } => vec![player, position],
            BlockKind::KickPlayer { player, reason } => vec![player, reason],
            BlockKind::ScrollTo { x, y, .. } => vec![x, y],
            BlockKind::ShakeCamera { intensity, seconds } => vec![intensity, seconds],
            BlockKind::SetAnimatorLayerWeight {
                weight, seconds, ..
            } => vec![weight, seconds],
            BlockKind::SpawnDecal {
                texture,
                position,
                facing,
                size,
                seconds,
            } => vec![texture, position, facing, size, seconds],
            BlockKind::Local { values, .. } | BlockKind::Return { values } => {
                values.iter().collect()
            }
            BlockKind::GenericFor { values, .. } => values.iter().collect(),
            BlockKind::Assign { targets, values } => targets.iter().chain(values.iter()).collect(),
            BlockKind::CompoundAssign { target, value, .. } => vec![target, value],
            BlockKind::CallFunction {
                function: first,
                args,
            }
            | BlockKind::CallMethod {
                object: first,
                args,
                ..
            } => std::iter::once(first).chain(args.iter()).collect(),
            BlockKind::OnSignal { signal: e, .. }
            | BlockKind::Connect { signal: e, .. }
            | BlockKind::While { condition: e, .. }
            | BlockKind::RepeatLoop { condition: e, .. } => vec![e],
            BlockKind::NumericFor {
                start, stop, step, ..
            } => {
                let mut slots = vec![start, stop];
                slots.extend(step.as_ref());
                slots
            }
            BlockKind::ForEachPartInRadius { center, radius, .. } => vec![center, radius],
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Expression {
    // Values typed into a slot.
    Number {
        value: f64,
    },
    Text {
        value: String,
    },
    Boolean {
        value: bool,
    },
    Vector3 {
        x: f64,
        y: f64,
        z: f64,
    },
    Color3 {
        r: f64,
        g: f64,
        b: f64,
    },

    // Reporters.
    Variable {
        name: String,
    },
    Math {
        op: MathOp,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    /// `math.abs` and friends, and rounding.
    Function {
        function: MathFunction,
        value: Box<Expression>,
    },
    /// A whole number from `min` to `max`, both included.
    Random {
        min: Box<Expression>,
        max: Box<Expression>,
    },
    /// Any number from `min` up to `max`: `Random.new():NextNumber`.
    RandomNumber {
        min: Box<Expression>,
        max: Box<Expression>,
    },
    /// The Vector3 or Color3 `alpha` of the way from `from` to `to`.
    Lerp {
        from: Box<Expression>,
        to: Box<Expression>,
        alpha: Box<Expression>,
    },
    /// Two values as text, one after the other.
    Join {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Compare {
        op: Comparison,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Logic {
        op: LogicOp,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Not {
        value: Box<Expression>,
    },
    /// An object's property, such as a part's `Position`.
    Property {
        target: String,
        property: String,
    },
    /// One of an object's attributes (`GetAttribute`), nil when it has none
    /// by that name.
    Attribute {
        target: String,
        attribute: String,
    },
    /// `workspace:Raycast(origin, direction)`.
    Raycast {
        origin: Box<Expression>,
        direction: Box<Expression>,
    },
    /// `workspace:Raycast(origin, direction, params)`, passing by `ignore`
    /// and everything inside it, such as the shooter's own character.
    RaycastIgnoring {
        origin: Box<Expression>,
        direction: Box<Expression>,
        ignore: String,
    },
    /// The part that touched, inside "when touched".
    TouchingPart,
    /// The part under the mouse: `LocalPlayer:GetMouse().Target`, the
    /// camera's ray through the mouse cast into the world. Client only.
    PartUnderMouse,
    /// Whether a key is held down. Client only.
    KeyDown {
        key: String,
    },
    /// An Animator's Bool parameter (`animator:GetBool`).
    AnimatorBool {
        animator: String,
        parameter: String,
    },
    /// An Animator's Number parameter (`animator:GetNumber`).
    AnimatorNumber {
        animator: String,
        parameter: String,
    },
    /// How much an Animator's layer counts now
    /// (`animator:GetLayerWeight`, Phase 78).
    AnimatorLayerWeight {
        animator: String,
        layer: String,
    },
    /// Whether `value` (an Instance) is of class `class` or inherits from
    /// it: `child:IsA("BasePart")`.
    IsA {
        value: Box<Expression>,
        class: String,
    },
    /// Whether `value` is on this device:
    /// `value.StreamingState == Enum.StreamingState.StreamedIn` (Phase 80).
    IsStreamedIn {
        value: Box<Expression>,
    },
    /// A row of a data table, by its key:
    /// `DataService:GetTable("weapons"):Get(key)`. Nil when there's none.
    DataRow {
        table: String,
        key: Box<Expression>,
    },
    /// The point `t` of the way along a Spline (`spline:GetPositionAt(t)`,
    /// Phase 73).
    SplinePosition {
        spline: String,
        t: Box<Expression>,
    },
    /// How far along a Spline its point nearest `position` is
    /// (`spline:GetClosestDistance(position)`).
    SplineClosestDistance {
        spline: String,
        position: Box<Expression>,
    },
    /// A row's value in one column: `row.Damage`.
    DataValue {
        column: String,
        row: Box<Expression>,
    },

    // Luau's own values (see `BlockKind::RunScript`).
    Nil,
    /// `...`.
    Vararg,
    /// A name, or a name and fields: `waiting`, `row.Binding.Text`.
    Name {
        name: String,
    },
    /// `object.name`, for an object that isn't a plain name.
    Field {
        object: Box<Expression>,
        name: String,
    },
    /// `object[key]`.
    Index {
        object: Box<Expression>,
        key: Box<Expression>,
    },
    /// `function(args)`.
    Call {
        function: Box<Expression>,
        args: Vec<Expression>,
    },
    /// `object:method(args)`.
    MethodCall {
        object: Box<Expression>,
        method: String,
        args: Vec<Expression>,
    },
    /// `function(params) ... end`. Its blocks are shown as a mouth of the
    /// block it's in: see [`BlockKind::bodies`].
    Closure {
        params: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        generics: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        returns: String,
        body: Vec<Block>,
    },
    /// `{ ... }`, on one line or one field to a line.
    Table {
        fields: Vec<TableField>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        multiline: bool,
    },
    /// An operator with no block of its own, or used on values the block
    /// wouldn't take: `..`, `//`, `a or 5`.
    Binary {
        op: String,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    /// `-value`, `#value` or `not value`.
    Unary {
        op: String,
        value: Box<Expression>,
    },
    /// `(value)`, kept where the brackets change what it means.
    Paren {
        value: Box<Expression>,
    },
    /// `` `{a} and {b}` ``: the text around the values, as written, which
    /// is one more piece than there are values.
    Interpolated {
        parts: Vec<String>,
        values: Vec<Expression>,
    },
    /// `if a then b elseif c then d else e`, as
    /// `[a, b, c, d, e]`.
    IfValue {
        parts: Vec<Expression>,
    },
    /// `value :: Type`, with the type as written.
    Cast {
        value: Box<Expression>,
        annotation: String,
    },

    /// Lua that has no block, kept word for word from a script switched
    /// from Code to Blocks. Pointed, so it fits any slot.
    Code {
        code: String,
    },
}

/// One field of an [`Expression::Table`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TableField {
    /// `name = value`.
    Named { name: String, value: Expression },
    /// `[key] = value`.
    Keyed { key: Expression, value: Expression },
    /// `value`.
    Item { value: Expression },
}

impl Expression {
    pub fn shape(&self) -> BlockShape {
        match self {
            Expression::Boolean { .. }
            | Expression::Compare { .. }
            | Expression::Logic { .. }
            | Expression::Not { .. }
            | Expression::KeyDown { .. }
            | Expression::AnimatorBool { .. }
            | Expression::IsA { .. }
            | Expression::IsStreamedIn { .. }
            | Expression::Code { .. } => BlockShape::Boolean,
            // Any value can be a condition in Luau, so what can't be told
            // apart from one fits a condition.
            expression if expression.is_untyped() => BlockShape::Boolean,
            _ => BlockShape::Value,
        }
    }

    /// A value that could be anything until the script runs.
    pub fn is_untyped(&self) -> bool {
        matches!(
            self,
            Expression::Nil
                | Expression::Vararg
                | Expression::Name { .. }
                | Expression::Field { .. }
                | Expression::Index { .. }
                | Expression::Call { .. }
                | Expression::MethodCall { .. }
                | Expression::Binary { .. }
                | Expression::Unary { .. }
                | Expression::Paren { .. }
                | Expression::IfValue { .. }
                | Expression::Cast { .. }
        )
    }

    /// Adds the bodies of the functions inside this value.
    fn closure_bodies<'a>(&'a self, bodies: &mut Vec<&'a Vec<Block>>) {
        if let Expression::Closure { body, .. } = self {
            bodies.push(body);
        }
        for child in self.children() {
            child.closure_bodies(bodies);
        }
    }

    fn closure_bodies_mut<'a>(&'a mut self, bodies: &mut Vec<&'a mut Vec<Block>>) {
        if let Expression::Closure { body, .. } = self {
            bodies.push(body);
            return;
        }
        for child in self.children_mut() {
            child.closure_bodies_mut(bodies);
        }
    }

    /// The list of inputs this value can have more or fewer of, if it has
    /// one: a call's arguments.
    pub fn input_list_mut(&mut self) -> Option<&mut Vec<Expression>> {
        match self {
            Expression::Call { args, .. } | Expression::MethodCall { args, .. } => Some(args),
            _ => None,
        }
    }

    /// A value typed in, rather than a reporter block.
    pub fn is_literal(&self) -> bool {
        matches!(
            self,
            Expression::Number { .. }
                | Expression::Text { .. }
                | Expression::Boolean { .. }
                | Expression::Vector3 { .. }
                | Expression::Color3 { .. }
                | Expression::Nil
                | Expression::Vararg
                | Expression::Name { .. }
                | Expression::Closure { .. }
        )
    }

    /// Whether this reporter only works in client block scripts.
    pub fn client_only(&self) -> bool {
        matches!(
            self,
            Expression::KeyDown { .. } | Expression::PartUnderMouse
        )
    }

    /// Like [`Expression::children`], for changing them.
    pub fn children_mut(&mut self) -> Vec<&mut Expression> {
        match self {
            Expression::Math { left, right, .. }
            | Expression::Join { left, right }
            | Expression::Compare { left, right, .. }
            | Expression::Logic { left, right, .. } => vec![left, right],
            Expression::Random { min, max } | Expression::RandomNumber { min, max } => {
                vec![min, max]
            }
            Expression::Lerp { from, to, alpha } => vec![from, to, alpha],
            Expression::Raycast { origin, direction }
            | Expression::RaycastIgnoring {
                origin, direction, ..
            } => vec![origin, direction],
            Expression::Function { value, .. }
            | Expression::Not { value }
            | Expression::IsA { value, .. }
            | Expression::IsStreamedIn { value }
            | Expression::DataRow { key: value, .. }
            | Expression::SplinePosition { t: value, .. }
            | Expression::SplineClosestDistance {
                position: value, ..
            }
            | Expression::DataValue { row: value, .. }
            | Expression::Field { object: value, .. }
            | Expression::Unary { value, .. }
            | Expression::Paren { value }
            | Expression::Cast { value, .. } => vec![value],
            Expression::Index { object, key } => vec![object, key],
            Expression::Binary { left, right, .. } => vec![left, right],
            Expression::Call {
                function: first,
                args,
            }
            | Expression::MethodCall {
                object: first,
                args,
                ..
            } => std::iter::once(first.as_mut())
                .chain(args.iter_mut())
                .collect(),
            Expression::Table { fields, .. } => {
                let mut slots = Vec::new();
                for field in fields {
                    match field {
                        TableField::Named { value, .. } | TableField::Item { value } => {
                            slots.push(value);
                        }
                        TableField::Keyed { key, value } => {
                            slots.push(key);
                            slots.push(value);
                        }
                    }
                }
                slots
            }
            Expression::Interpolated { values: list, .. } | Expression::IfValue { parts: list } => {
                list.iter_mut().collect()
            }
            _ => Vec::new(),
        }
    }

    /// The reporters directly inside this one.
    pub fn children(&self) -> Vec<&Expression> {
        match self {
            Expression::Math { left, right, .. }
            | Expression::Join { left, right }
            | Expression::Compare { left, right, .. }
            | Expression::Logic { left, right, .. } => vec![left, right],
            Expression::Random { min, max } | Expression::RandomNumber { min, max } => {
                vec![min, max]
            }
            Expression::Lerp { from, to, alpha } => vec![from, to, alpha],
            Expression::Raycast { origin, direction }
            | Expression::RaycastIgnoring {
                origin, direction, ..
            } => vec![origin, direction],
            Expression::Function { value, .. }
            | Expression::Not { value }
            | Expression::IsStreamedIn { value }
            | Expression::IsA { value, .. }
            | Expression::DataRow { key: value, .. }
            | Expression::SplinePosition { t: value, .. }
            | Expression::SplineClosestDistance {
                position: value, ..
            }
            | Expression::DataValue { row: value, .. }
            | Expression::Field { object: value, .. }
            | Expression::Unary { value, .. }
            | Expression::Paren { value }
            | Expression::Cast { value, .. } => vec![value],
            Expression::Index { object, key } => vec![object, key],
            Expression::Binary { left, right, .. } => vec![left, right],
            Expression::Call {
                function: first,
                args,
            }
            | Expression::MethodCall {
                object: first,
                args,
                ..
            } => std::iter::once(first.as_ref()).chain(args.iter()).collect(),
            Expression::Table { fields, .. } => {
                let mut slots = Vec::new();
                for field in fields {
                    match field {
                        TableField::Named { value, .. } | TableField::Item { value } => {
                            slots.push(value);
                        }
                        TableField::Keyed { key, value } => {
                            slots.push(key);
                            slots.push(value);
                        }
                    }
                }
                slots
            }
            Expression::Interpolated { values: list, .. } | Expression::IfValue { parts: list } => {
                list.iter().collect()
            }
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl Comparison {
    pub const ALL: [Comparison; 6] = [
        Comparison::Equal,
        Comparison::NotEqual,
        Comparison::Less,
        Comparison::LessOrEqual,
        Comparison::Greater,
        Comparison::GreaterOrEqual,
    ];

    pub fn lua(self) -> &'static str {
        match self {
            Self::Equal => "==",
            Self::NotEqual => "~=",
            Self::Less => "<",
            Self::LessOrEqual => "<=",
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
        }
    }

    /// The symbol a block shows.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Equal => "=",
            Self::NotEqual => "≠",
            Self::Less => "<",
            Self::LessOrEqual => "≤",
            Self::Greater => ">",
            Self::GreaterOrEqual => "≥",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
}

impl MathOp {
    pub const ALL: [MathOp; 6] = [
        MathOp::Add,
        MathOp::Subtract,
        MathOp::Multiply,
        MathOp::Divide,
        MathOp::Modulo,
        MathOp::Power,
    ];

    pub fn lua(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Modulo => "%",
            Self::Power => "^",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "−",
            Self::Multiply => "×",
            Self::Divide => "÷",
            Self::Modulo => "mod",
            Self::Power => "^",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogicOp {
    And,
    Or,
}

impl LogicOp {
    pub fn lua(self) -> &'static str {
        match self {
            Self::And => "and",
            Self::Or => "or",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathFunction {
    Abs,
    Floor,
    Ceil,
    Round,
    Sqrt,
}

impl MathFunction {
    pub const ALL: [MathFunction; 5] = [
        MathFunction::Abs,
        MathFunction::Floor,
        MathFunction::Ceil,
        MathFunction::Round,
        MathFunction::Sqrt,
    ];

    /// The name a block shows.
    pub fn name(self) -> &'static str {
        match self {
            Self::Abs => "abs",
            Self::Floor => "floor",
            Self::Ceil => "ceiling",
            Self::Round => "round",
            Self::Sqrt => "sqrt",
        }
    }
}

#[cfg(test)]
mod ui_graph_tests;
