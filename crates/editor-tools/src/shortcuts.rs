//! Every keyboard shortcut in the editor, as data (design system V16): what
//! it does, its keys, and where it works. Handlers ask this table
//! ([`consume`], [`consume_first`], [`pressed`], [`count`]) instead of
//! naming keys; Settings ▸ Shortcuts lists it by [`Context`], and menus and
//! the command palette show keys from it ([`Action::menu_text`],
//! [`for_command`]). Rebinding (V23) only has to change [`Action::keys`].
//!
//! Not here: the conventions of fields, menus and dialogs (Enter confirms,
//! Escape cancels or closes, Tab moves on), which work the same everywhere,
//! and the keys a game gets during Play.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

use crate::gizmo::Tool;
use crate::terrain_tool::TerrainTool;

const NONE: Modifiers = Modifiers::NONE;
const SHIFT: Modifiers = Modifiers::SHIFT;
const ALT: Modifiers = Modifiers::ALT;
/// Ctrl, or Cmd on macOS.
const CMD: Modifiers = Modifiers::COMMAND;
const CMD_SHIFT: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);

/// A `&'static` list of chords, written `keys![CMD S, NONE F2]`. (A
/// `const` item is needed: a `const fn` call isn't promoted to `'static`
/// on its own.)
macro_rules! keys {
    ($($modifiers:ident $key:ident),* $(,)?) => {{
        const KEYS: &[KeyboardShortcut] = &[$(KeyboardShortcut::new($modifiers, Key::$key)),*];
        KEYS
    }};
}

/// Where a shortcut works. Settings ▸ Shortcuts has a group for each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Context {
    Everywhere,
    /// Editing the scene, while no text is being typed: undo, the
    /// selection, the tools.
    Scene,
    /// Once the Explorer is clicked or tabbed into.
    Explorer,
    Camera,
    /// While the Terrain Studio has a tool.
    Terrain,
    /// The Terrain Studio's brush, and the Model Studio's Paint and Weights
    /// brushes.
    Brushes,
    /// Typing in the Console's command line.
    Console,
    /// Over the Animation tab's clip or the Sequencer's sequence.
    Animation,
    /// Over a graph: a block script's Graph view, a material graph, a UI
    /// Event Graph, the Animation tab's state machine.
    Graphs,
    UiDesigner,
    /// The UI Designer's Pages tab.
    UiPages,
    /// Over the Model Studio's view, in Object or Edit mode.
    ModelStudio,
    /// While a move, turn or scale in the Model Studio follows the mouse.
    StudioMoving,
    /// While a Model Studio tool's amount is being typed.
    StudioValue,
    UvEditor,
    /// While a move in the UV editor follows the mouse.
    UvMoving,
    /// Armature and Pose modes.
    Rig,
    /// Paint mode.
    Paint,
}

impl Context {
    pub const ALL: [Context; 18] = [
        Context::Everywhere,
        Context::Scene,
        Context::Explorer,
        Context::Camera,
        Context::Terrain,
        Context::Brushes,
        Context::Console,
        Context::Animation,
        Context::Graphs,
        Context::UiDesigner,
        Context::UiPages,
        Context::ModelStudio,
        Context::StudioMoving,
        Context::StudioValue,
        Context::UvEditor,
        Context::UvMoving,
        Context::Rig,
        Context::Paint,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Context::Everywhere => "Everywhere",
            Context::Scene => "Editing the scene",
            Context::Explorer => "Explorer (once clicked)",
            Context::Camera => "Viewport camera",
            Context::Terrain => "Terrain Studio",
            Context::Brushes => "Brushes",
            Context::Console => "Console command line",
            Context::Animation => "Animation and Sequencer",
            Context::Graphs => "Graphs",
            Context::UiDesigner => "UI Designer",
            Context::UiPages => "UI Designer's Pages",
            Context::ModelStudio => "Model Studio",
            Context::StudioMoving => "Model Studio, while moving",
            Context::StudioValue => "Model Studio, while typing an amount",
            Context::UvEditor => "Model Studio's UV editor",
            Context::UvMoving => "UV editor, while moving",
            Context::Rig => "Model Studio's Armature and Pose modes",
            Context::Paint => "Model Studio's Paint mode",
        }
    }

    /// When it applies, for the group's subtitle.
    pub fn about(self) -> &'static str {
        match self {
            Context::Everywhere => "In any panel, even while typing.",
            Context::Scene => "While nothing is being typed.",
            Context::Explorer => "After clicking in the Explorer or tabbing to it.",
            Context::Camera => "Over the Scene view.",
            Context::Terrain => "While a terrain tool is picked.",
            Context::Brushes => "The terrain brush, and Paint and Weights.",
            Context::Console => "While its command line has the cursor.",
            Context::Animation => "With the mouse over a clip or a sequence.",
            Context::Graphs => {
                "Over a block script's Graph view, a material, a UI Event Graph or a state \
                 machine."
            }
            Context::UiDesigner => "Over the artboard.",
            Context::UiPages => "On the Pages tab, while nothing is being typed.",
            Context::ModelStudio => "Over the 3D view, in Object or Edit mode.",
            Context::StudioMoving => "While a move, turn or scale follows the mouse.",
            Context::StudioValue => "While a tool's amount is being typed.",
            Context::UvEditor => "Over the UV editor.",
            Context::UvMoving => "While a move in the UV editor follows the mouse.",
            Context::Rig => "Over the 3D view.",
            Context::Paint => "Over the 3D view.",
        }
    }

    /// Whether shortcuts of both can be live at once, so they mustn't share
    /// keys unless one [wins](Self::wins_over). Everywhere is live with all.
    #[cfg(test)] // Checked by the clash test; rebinding (V23) will use it too.
    pub fn overlaps(self, other: Context) -> bool {
        use Context::*;
        let pair = |a, b| (self == a && other == b) || (self == b && other == a);
        self == other
            || self == Everywhere
            || other == Everywhere
            || [
                Explorer, Camera, Terrain, Animation, Graphs, UiDesigner, UiPages,
            ]
            .into_iter()
            .any(|live| pair(Scene, live))
            || [UiDesigner, UiPages, Terrain]
                .into_iter()
                .any(|live| pair(Explorer, live))
            || pair(Terrain, Brushes)
    }

    /// Whether this one's keys are taken first where both are live, so a
    /// key they share does this one's action. As the handlers run: the
    /// Terrain Studio's keys before the scene's; Enter for the Anchor and
    /// Angular tools before the Explorer's rename; the Explorer's arrows
    /// before the UI Designer's; a clip's and a block graph's keys instead
    /// of the scene's.
    #[cfg(test)] // Checked by the clash test; rebinding (V23) will use it too.
    pub fn wins_over(self, other: Context) -> bool {
        use Context::*;
        matches!(
            (self, other),
            (Terrain, Scene)
                | (Scene, Explorer)
                | (Explorer, UiDesigner | UiPages)
                | (Animation | Graphs, Scene)
        )
    }
}

/// Something a shortcut does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    // Everywhere.
    Save,
    Settings,
    Search,
    Palette,
    QuickOpen,
    FindInScripts,
    Play,
    Stop,
    /// While paused at a breakpoint (roadmap G6).
    StepOver,
    StepInto,
    StepOut,
    DevConsole,

    // Editing the scene.
    Undo,
    Redo,
    Duplicate,
    Rename,
    Delete,
    Tool(Tool),
    /// Escape: back to Select, and the Constraint tool put down.
    PutToolDown,
    /// Enter: the Anchor tool flips Anchored, the Angular tool extends.
    UseTool,

    // Explorer.
    ExplorerMove,
    ExplorerExtend,
    ExplorerClose,
    ExplorerOpen,
    ExplorerEnds,
    ExplorerRename,

    // Viewport camera: the mouse, and W A S D E Q while looking around.
    LookAround,
    Fly,
    FlySlower,
    Pan,
    Orbit,
    FlySpeed,

    // Terrain Studio and brushes.
    TerrainTool(TerrainTool),
    PutBrushDown,
    BrushSmaller,
    BrushBigger,

    // Console.
    ConsoleRun,
    ConsoleComplete,
    ConsoleHistory,

    // Animation clip.
    /// Play, pause or resume the open clip.
    PlayPause,
    PreviousFrame,
    NextFrame,
    /// Ten frames back.
    PreviousFrames,
    NextFrames,
    ClipStart,
    ClipEnd,
    KeySelected,
    DeleteKeys,
    CopyKeys,
    CutKeys,
    PasteKeys,
    FrameCurves,

    // Graphs.
    DeleteNodes,
    DuplicateNodes,
    SelectAllNodes,
    ClearNodeSelection,

    // UI Designer.
    AddToSelection,
    BoxSelect,
    Nudge,
    NudgeTen,
    CopyElements,
    PasteElements,
    Bold,
    Italic,
    Underline,
    PreviousSlide,
    NextSlide,

    // Model Studio.
    StudioMode,
    SelectMode,
    Grab,
    Turn,
    Resize,
    SelectLinked,
    SelectAll,
    GrowSelection,
    ShrinkSelection,
    Proportional,
    CursorHome,
    UvMenu,
    Extrude,
    Inset,
    Knife,
    Merge,
    DeleteMesh,
    Fill,
    Bevel,
    BevelVertices,
    LoopCut,
    NormalsOutside,
    NormalsInside,
    FlipNormals,

    // Model Studio, while moving.
    LockAxis,
    EdgeSlide,
    ProportionalRadius,
    EraseDigit,
    FinishMove,
    CancelMove,

    // Model Studio, while typing an amount.
    EraseAmountDigit,
    ApplyAmount,
    CancelAmount,

    // UV editor.
    Pin,
    Unpin,
    Stitch,
    SelectAllUvs,
    GrabUvs,
    TurnUvs,
    ResizeUvs,
    LockUvAxis,
    FinishUvMove,
    CancelUvMove,

    // Armature and Pose.
    ExtrudeBone,
    MoveBone,
    DeleteBone,
    TurnBone,
    FinishBoneMove,
    CancelBoneMove,

    // Paint.
    SwapDrawErase,
}

impl Action {
    /// Every action, in Settings ▸ Shortcuts' order.
    pub fn all() -> Vec<Action> {
        use Action::*;
        let mut all = vec![
            Save,
            Settings,
            Search,
            Palette,
            QuickOpen,
            FindInScripts,
            Play,
            Stop,
            StepOver,
            StepInto,
            StepOut,
            DevConsole,
            Undo,
            Redo,
            Duplicate,
            Rename,
            Delete,
        ];
        all.extend(
            crate::gizmo::Tool::ALL
                .into_iter()
                .filter(|tool| !Action::Tool(*tool).keys().is_empty())
                .map(Tool),
        );
        all.extend([
            PutToolDown,
            UseTool,
            ExplorerMove,
            ExplorerExtend,
            ExplorerClose,
            ExplorerOpen,
            ExplorerEnds,
            ExplorerRename,
            LookAround,
            Fly,
            FlySlower,
            Pan,
            Orbit,
            FlySpeed,
        ]);
        all.extend(
            crate::terrain_tool::TerrainTool::all()
                .filter(|tool| !Action::TerrainTool(*tool).keys().is_empty())
                .map(TerrainTool),
        );
        all.extend([
            PutBrushDown,
            BrushSmaller,
            BrushBigger,
            ConsoleRun,
            ConsoleComplete,
            ConsoleHistory,
            PlayPause,
            PreviousFrame,
            NextFrame,
            PreviousFrames,
            NextFrames,
            ClipStart,
            ClipEnd,
            KeySelected,
            DeleteKeys,
            CopyKeys,
            CutKeys,
            PasteKeys,
            FrameCurves,
            DeleteNodes,
            DuplicateNodes,
            SelectAllNodes,
            ClearNodeSelection,
            AddToSelection,
            BoxSelect,
            Nudge,
            NudgeTen,
            CopyElements,
            PasteElements,
            Bold,
            Italic,
            Underline,
            PreviousSlide,
            NextSlide,
            StudioMode,
            SelectMode,
            Grab,
            Turn,
            Resize,
            SelectLinked,
            SelectAll,
            GrowSelection,
            ShrinkSelection,
            Proportional,
            CursorHome,
            UvMenu,
            Extrude,
            Inset,
            Knife,
            Merge,
            DeleteMesh,
            Fill,
            Bevel,
            BevelVertices,
            LoopCut,
            NormalsOutside,
            NormalsInside,
            FlipNormals,
            LockAxis,
            EdgeSlide,
            ProportionalRadius,
            EraseDigit,
            FinishMove,
            CancelMove,
            EraseAmountDigit,
            ApplyAmount,
            CancelAmount,
            Pin,
            Unpin,
            Stitch,
            SelectAllUvs,
            GrabUvs,
            TurnUvs,
            ResizeUvs,
            LockUvAxis,
            FinishUvMove,
            CancelUvMove,
            ExtrudeBone,
            MoveBone,
            DeleteBone,
            TurnBone,
            FinishBoneMove,
            CancelBoneMove,
            SwapDrawErase,
        ]);
        all
    }

    /// Its keys: any one of them does it. Empty for what the mouse does
    /// (see [`gesture`](Self::gesture)).
    pub fn keys(self) -> &'static [KeyboardShortcut] {
        use Action::*;
        match self {
            Save => keys![CMD S],
            Settings => keys![CMD Comma],
            Search => keys![CMD K],
            Palette => keys![CMD_SHIFT P],
            QuickOpen => keys![CMD P],
            FindInScripts => keys![CMD_SHIFT F],
            // F5 plays and Shift+F5 stops, as in Roblox Studio.
            Play => keys![NONE F5],
            Stop => keys![SHIFT F5],
            // Stepping while paused, as in Roblox Studio and most debuggers.
            StepOver => keys![NONE F10],
            StepInto => keys![NONE F11],
            StepOut => keys![SHIFT F11],
            DevConsole => keys![NONE F9],

            Undo => keys![CMD Z],
            // Ctrl+Y as on Windows, and Ctrl+Shift+Z.
            Redo => keys![CMD Y, CMD_SHIFT Z],
            Duplicate => keys![CMD D],
            // F2, as in Studio and most file managers.
            Rename => keys![NONE F2],
            // On macOS the key labelled "delete" is Backspace.
            Delete if cfg!(target_os = "macos") => keys![NONE Backspace, NONE Delete],
            Delete => keys![NONE Delete],
            Tool(tool) => tool_keys(tool),
            PutToolDown => keys![NONE Escape],
            UseTool => keys![NONE Enter],

            ExplorerMove => keys![NONE ArrowUp, NONE ArrowDown],
            ExplorerExtend => keys![
                SHIFT ArrowUp,
                SHIFT ArrowDown,
                SHIFT ArrowLeft,
                SHIFT ArrowRight,
                SHIFT Home,
                SHIFT End,
            ],
            ExplorerClose => keys![NONE ArrowLeft],
            ExplorerOpen => keys![NONE ArrowRight],
            ExplorerEnds => keys![NONE Home, NONE End],
            ExplorerRename => keys![NONE Enter],

            LookAround | Pan | Orbit | FlySpeed | FlySlower => &[],
            Fly => keys![NONE W, NONE A, NONE S, NONE D, NONE E, NONE Q],

            TerrainTool(tool) => terrain_keys(tool),
            PutBrushDown => keys![NONE Escape],
            BrushSmaller => keys![NONE OpenBracket],
            BrushBigger => keys![NONE CloseBracket],

            ConsoleRun => keys![NONE Enter],
            ConsoleComplete => keys![NONE Tab],
            ConsoleHistory => keys![NONE ArrowUp, NONE ArrowDown],

            PlayPause => keys![NONE Space],
            PreviousFrame => keys![NONE ArrowLeft],
            NextFrame => keys![NONE ArrowRight],
            PreviousFrames => keys![SHIFT ArrowLeft],
            NextFrames => keys![SHIFT ArrowRight],
            ClipStart => keys![NONE Home],
            ClipEnd => keys![NONE End],
            KeySelected => keys![NONE K],
            DeleteKeys => keys![NONE Delete, NONE Backspace],
            CopyKeys => keys![CMD C],
            CutKeys => keys![CMD X],
            PasteKeys => keys![CMD V],
            FrameCurves => keys![NONE F],

            DeleteNodes => keys![NONE Delete, NONE Backspace],
            DuplicateNodes => keys![CMD D],
            SelectAllNodes => keys![CMD A],
            ClearNodeSelection => keys![NONE Escape],

            AddToSelection | BoxSelect => &[],
            Nudge => keys![NONE ArrowLeft, NONE ArrowRight, NONE ArrowUp, NONE ArrowDown],
            NudgeTen => keys![
                SHIFT ArrowLeft,
                SHIFT ArrowRight,
                SHIFT ArrowUp,
                SHIFT ArrowDown,
            ],
            CopyElements => keys![CMD C],
            PasteElements => keys![CMD V],
            Bold => keys![CMD B],
            Italic => keys![CMD I],
            Underline => keys![CMD U],
            PreviousSlide => keys![NONE ArrowLeft],
            NextSlide => keys![NONE ArrowRight],

            StudioMode => keys![NONE Tab],
            SelectMode => keys![NONE Num1, NONE Num2, NONE Num3],
            Grab => keys![NONE G],
            Turn => keys![NONE R],
            Resize => keys![NONE S],
            SelectLinked => keys![NONE L],
            SelectAll => keys![NONE A],
            GrowSelection => keys![CMD Plus, CMD Equals],
            ShrinkSelection => keys![CMD Minus],
            Proportional => keys![NONE O],
            CursorHome => keys![SHIFT C],
            UvMenu => keys![NONE U],
            Extrude => keys![NONE E],
            Inset => keys![NONE I],
            Knife => keys![NONE K],
            Merge => keys![NONE M],
            DeleteMesh => keys![NONE X, NONE Delete],
            Fill => keys![NONE F],
            Bevel => keys![CMD B],
            BevelVertices => keys![CMD_SHIFT B],
            LoopCut => keys![CMD R],
            NormalsOutside => keys![SHIFT N],
            NormalsInside => keys![CMD_SHIFT N],
            FlipNormals => keys![ALT N],

            LockAxis => keys![NONE X, NONE Y, NONE Z],
            EdgeSlide => keys![NONE G],
            ProportionalRadius => keys![NONE PageUp, NONE PageDown],
            EraseDigit | EraseAmountDigit => keys![NONE Backspace],
            FinishMove | FinishUvMove | FinishBoneMove => keys![NONE Enter],
            CancelMove | CancelAmount | CancelUvMove | CancelBoneMove => {
                keys![NONE Escape]
            }
            ApplyAmount => keys![NONE Enter, NONE Space],

            Pin => keys![NONE P],
            Unpin => keys![ALT P],
            Stitch => keys![NONE V],
            SelectAllUvs => keys![NONE A],
            GrabUvs => keys![NONE G],
            TurnUvs => keys![NONE R],
            ResizeUvs => keys![NONE S],
            LockUvAxis => keys![NONE X, NONE Y],

            ExtrudeBone => keys![NONE E],
            MoveBone => keys![NONE G],
            DeleteBone => keys![NONE X, NONE Delete],
            TurnBone => keys![NONE R],

            SwapDrawErase => keys![NONE X],
        }
    }

    /// What the mouse does it with, for an action without keys or one
    /// whose keys work only during a drag.
    pub fn gesture(self) -> Option<&'static str> {
        Some(match self {
            Action::LookAround => "Right-drag",
            Action::Fly => "While right-dragging",
            Action::FlySlower => "Shift while flying",
            Action::Pan => "Middle-drag",
            Action::Orbit => "Alt + left-drag",
            Action::FlySpeed => "Shift + wheel",
            Action::AddToSelection => "Shift or Ctrl + click",
            Action::BoxSelect => "Drag on empty artboard",
            Action::ProportionalRadius => "or the wheel",
            _ => return None,
        })
    }

    pub fn context(self) -> Context {
        use Action::*;
        match self {
            Save | Settings | Search | Palette | QuickOpen | FindInScripts | Play | Stop
            | StepOver | StepInto | StepOut | DevConsole => Context::Everywhere,
            Undo | Redo | Duplicate | Rename | Delete | Tool(_) | PutToolDown | UseTool => {
                Context::Scene
            }
            ExplorerMove | ExplorerExtend | ExplorerClose | ExplorerOpen | ExplorerEnds
            | ExplorerRename => Context::Explorer,
            LookAround | Fly | FlySlower | Pan | Orbit | FlySpeed => Context::Camera,
            TerrainTool(_) | PutBrushDown => Context::Terrain,
            BrushSmaller | BrushBigger => Context::Brushes,
            ConsoleRun | ConsoleComplete | ConsoleHistory => Context::Console,
            PlayPause | PreviousFrame | NextFrame | PreviousFrames | NextFrames | ClipStart
            | ClipEnd | KeySelected | DeleteKeys | CopyKeys | CutKeys | PasteKeys | FrameCurves => {
                Context::Animation
            }
            DeleteNodes | DuplicateNodes | SelectAllNodes | ClearNodeSelection => Context::Graphs,
            AddToSelection | BoxSelect | Nudge | NudgeTen | CopyElements | PasteElements | Bold
            | Italic | Underline => Context::UiDesigner,
            PreviousSlide | NextSlide => Context::UiPages,
            StudioMode | SelectMode | Grab | Turn | Resize | SelectLinked | SelectAll
            | GrowSelection | ShrinkSelection | Proportional | CursorHome | UvMenu | Extrude
            | Inset | Knife | Merge | DeleteMesh | Fill | Bevel | BevelVertices | LoopCut
            | NormalsOutside | NormalsInside | FlipNormals => Context::ModelStudio,
            LockAxis | EdgeSlide | ProportionalRadius | EraseDigit | FinishMove | CancelMove => {
                Context::StudioMoving
            }
            EraseAmountDigit | ApplyAmount | CancelAmount => Context::StudioValue,
            Pin | Unpin | Stitch | SelectAllUvs | GrabUvs | TurnUvs | ResizeUvs => {
                Context::UvEditor
            }
            LockUvAxis | FinishUvMove | CancelUvMove => Context::UvMoving,
            ExtrudeBone | MoveBone | DeleteBone | TurnBone | FinishBoneMove | CancelBoneMove => {
                Context::Rig
            }
            SwapDrawErase => Context::Paint,
        }
    }

    /// What it does, as Settings ▸ Shortcuts says it.
    pub fn label(self) -> String {
        use Action::*;
        let label = match self {
            Save => "Save",
            Settings => "Settings",
            Search => "Search everything",
            Palette => "Command palette",
            QuickOpen => "Quick Open",
            FindInScripts => "Find in Scripts",
            Play => "Play, or Continue",
            Stop => "Stop",
            StepOver => "Step Over, while paused",
            StepInto => "Step Into, while paused",
            StepOut => "Step Out, while paused",
            DevConsole => "Developer console, during Play",
            Undo => "Undo",
            Redo => "Redo",
            Duplicate => "Duplicate",
            Rename => "Rename",
            Delete => "Delete",
            Tool(tool) => return format!("{} tool", tool.label()),
            PutToolDown => "Back to Select, or put the Constraint tool down",
            UseTool => "Anchor tool: flip Anchored. Angular tool: extend",
            ExplorerMove => "Move the selection",
            ExplorerExtend => "Extend the selection",
            ExplorerClose => "Close, or go to the parent",
            ExplorerOpen => "Open, or go to the first child",
            ExplorerEnds => "First and last rows",
            ExplorerRename => "Rename",
            LookAround => "Look around",
            Fly => "Fly with W A S D, up with E, down with Q",
            FlySlower => "Fly slower",
            Pan => "Pan",
            Orbit => "Orbit",
            FlySpeed => "Fly speed",
            TerrainTool(tool) => return format!("{} tool", tool.label()),
            PutBrushDown => "Put the brush down",
            BrushSmaller => "Smaller brush",
            BrushBigger => "Bigger brush",
            ConsoleRun => "Run the command (Shift+Enter: new line)",
            ConsoleComplete => "Complete the name",
            ConsoleHistory => "Earlier and later commands",
            PlayPause => "Play, pause or resume the clip",
            PreviousFrame => "Back one frame",
            NextFrame => "On one frame",
            PreviousFrames => "Back ten frames",
            NextFrames => "On ten frames",
            ClipStart => "Go to the clip's start",
            ClipEnd => "Go to the clip's end",
            KeySelected => "Key the selected",
            DeleteKeys => "Delete the selected keys",
            CopyKeys => "Copy keys",
            CutKeys => "Cut keys",
            PasteKeys => "Paste keys",
            FrameCurves => "Fit the curves in view",
            DeleteNodes => "Delete the selected nodes",
            DuplicateNodes => "Duplicate the selected nodes",
            SelectAllNodes => "Select every node (material and UI graphs)",
            ClearNodeSelection => "Clear the selection",
            AddToSelection => "Add to the selection",
            BoxSelect => "Select with a box",
            Nudge => "Nudge 1 pixel",
            NudgeTen => "Nudge 10 pixels",
            CopyElements => "Copy elements",
            PasteElements => "Paste elements",
            Bold => "Bold text",
            Italic => "Italic text",
            Underline => "Underlined text",
            PreviousSlide => "Previous page",
            NextSlide => "Next page",
            StudioMode => "Object or Edit mode (and back from the other modes)",
            SelectMode => "Select vertices, edges or faces",
            Grab => "Move",
            Turn => "Turn",
            Resize => "Scale",
            SelectLinked => "Select linked",
            SelectAll => "Select all, or none",
            GrowSelection => "Grow the selection",
            ShrinkSelection => "Shrink the selection",
            Proportional => "Proportional editing on or off",
            CursorHome => "3D cursor to the middle",
            UvMenu => "UV menu (Edit mode)",
            Extrude => "Extrude (Edit mode)",
            Inset => "Inset (Edit mode)",
            Knife => "Knife (Edit mode)",
            Merge => "Merge (Edit mode)",
            DeleteMesh => "Delete (Edit mode)",
            Fill => "Fill (Edit mode)",
            Bevel => "Bevel (Edit mode)",
            BevelVertices => "Bevel vertices (Edit mode)",
            LoopCut => "Loop cut (Edit mode)",
            NormalsOutside => "Normals outside (Edit mode)",
            NormalsInside => "Normals inside (Edit mode)",
            FlipNormals => "Flip normals (Edit mode)",
            LockAxis => "Only along X, Y or Z (again: local, then free)",
            EdgeSlide => "Slide along edges instead",
            ProportionalRadius => "Proportional radius",
            EraseDigit | EraseAmountDigit => "Erase a typed digit",
            FinishMove | FinishUvMove | FinishBoneMove => "Finish",
            ApplyAmount => "Apply",
            CancelMove | CancelAmount | CancelUvMove | CancelBoneMove => "Cancel",
            Pin => "Pin",
            Unpin => "Unpin",
            Stitch => "Stitch",
            SelectAllUvs => "Select all, or none",
            GrabUvs => "Move",
            TurnUvs => "Turn",
            ResizeUvs => "Scale",
            LockUvAxis => "Only along U or V",
            ExtrudeBone => "Extrude a bone (Armature)",
            MoveBone => "Move the bone (Armature)",
            DeleteBone => "Delete the bone (Armature)",
            TurnBone => "Turn the bone (Pose)",
            SwapDrawErase => "Draw or Erase",
        };
        label.to_owned()
    }

    /// The palette command it runs, by the command's debug name, when it
    /// does what a command does.
    pub fn command(self) -> Option<String> {
        use Action::*;
        Some(match self {
            Save | Settings | Search | Palette | QuickOpen | FindInScripts | Play | Stop | Undo
            | Redo | Duplicate | Rename | Delete => format!("{self:?}"),
            Tool(tool) => format!("Tool({tool:?})"),
            TerrainTool(tool) => format!("TerrainTool({tool:?})"),
            _ => return None,
        })
    }

    /// Its first keys, as a menu item shows them.
    pub fn menu_text(self, ctx: &egui::Context) -> String {
        self.keys()
            .first()
            .map(|chord| chord_text(ctx, chord))
            .unwrap_or_default()
    }

    /// All its keys, as Settings ▸ Shortcuts shows them: chords with the
    /// same modifiers share them ("Shift+Up, Down"), and a gesture follows.
    pub fn text(self, ctx: &egui::Context) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut last: Option<Modifiers> = None;
        for chord in self.keys() {
            if last == Some(chord.modifiers) && chord.modifiers != NONE {
                parts.push(key_name(ctx, chord.logical_key));
            } else {
                parts.push(chord_text(ctx, chord));
            }
            last = Some(chord.modifiers);
        }
        let mut text = parts.join(", ");
        if let Some(gesture) = self.gesture() {
            if !text.is_empty() {
                text.push_str(", ");
            }
            text.push_str(gesture);
        }
        text
    }
}

fn tool_keys(tool: Tool) -> &'static [KeyboardShortcut] {
    // 1 to 5 for the tools with handles, as in Studio, and F3X's letters
    // for the building tools.
    match tool {
        Tool::Select => keys![NONE Num1],
        Tool::Move => keys![NONE Num2],
        Tool::Scale => keys![NONE Num3],
        Tool::Rotate => keys![NONE Num4],
        Tool::Transform => keys![NONE Num5],
        Tool::Paint => keys![NONE V],
        Tool::Material => keys![NONE N],
        Tool::Anchor => keys![NONE M],
        Tool::Lighting => keys![NONE U],
        Tool::Emission => keys![NONE P],
        // Not an F3X tool; R is free in F3X and in the camera keys.
        Tool::Angular => keys![NONE R],
    }
}

fn terrain_keys(tool: TerrainTool) -> &'static [KeyboardShortcut] {
    // 1 to 9, then 0.
    match tool {
        TerrainTool::Raise => keys![NONE Num1],
        TerrainTool::Smooth => keys![NONE Num2],
        TerrainTool::Flatten => keys![NONE Num3],
        TerrainTool::Sculpt => keys![NONE Num4],
        TerrainTool::Stamp => keys![NONE Num5],
        TerrainTool::Noise => keys![NONE Num6],
        TerrainTool::Paint => keys![NONE Num7],
        TerrainTool::Auto => keys![NONE Num8],
        TerrainTool::Generate => keys![NONE Num9],
        TerrainTool::Erode => keys![NONE Num0],
        TerrainTool::Foliage | TerrainTool::Masks => &[],
    }
}

/// One chord as egui writes it for this OS ("Ctrl+S", "⌘S"), with the
/// arrows, Delete and Escape by name and brackets by symbol.
fn chord_text(ctx: &egui::Context, chord: &KeyboardShortcut) -> String {
    let text = ctx.format_shortcut(chord);
    let key = chord.logical_key;
    match text
        .strip_suffix(key.name())
        .or_else(|| text.strip_suffix(key.symbol_or_name()))
    {
        Some(modifiers) => format!("{modifiers}{}", key_name(ctx, chord.logical_key)),
        None => text,
    }
}

fn key_name(ctx: &egui::Context, key: Key) -> String {
    let mac = ctx.os().is_mac();
    match key {
        Key::ArrowUp => "Up".into(),
        Key::ArrowDown => "Down".into(),
        Key::ArrowLeft => "Left".into(),
        Key::ArrowRight => "Right".into(),
        Key::Delete if !mac => "Del".into(),
        Key::Backspace if mac => "⌫".into(),
        Key::Escape => "Esc".into(),
        key => key.symbol_or_name().to_owned(),
    }
}

/// Takes `action`'s keys out of this frame's input; true if one was
/// pressed. Chords with more modifiers are tried first, since egui counts
/// Ctrl+Shift+Z as Ctrl+Z too.
pub fn consume(input: &mut egui::InputState, action: Action) -> bool {
    consume_first(input, &[action]).is_some()
}

/// The first of `actions` whose keys were pressed, taking them: chords with
/// more modifiers first, across all of them (Shift+F5 before F5), then in
/// the order given.
pub fn consume_first(input: &mut egui::InputState, actions: &[Action]) -> Option<Action> {
    let mut chords: Vec<(Action, &KeyboardShortcut)> = actions
        .iter()
        .flat_map(|&action| action.keys().iter().map(move |chord| (action, chord)))
        .collect();
    // A stable sort, so equal ones keep the order given.
    chords.sort_by_key(|(_, chord)| std::cmp::Reverse(modifier_count(chord.modifiers)));
    chords
        .into_iter()
        .find(|(_, chord)| input.consume_shortcut(chord))
        .map(|(action, _)| action)
}

/// How many times each of `action`'s keys was pressed this frame (held keys
/// repeat), taking them, in the order of [`Action::keys`].
pub fn count(input: &mut egui::InputState, action: Action) -> Vec<(Key, usize)> {
    action
        .keys()
        .iter()
        .map(|chord| {
            let times = input.count_and_consume_key(chord.modifiers, chord.logical_key);
            (chord.logical_key, times)
        })
        .collect()
}

/// Whether one of `action`'s keys was pressed this frame, leaving it in the
/// input. Shift and Alt held too still count, as with [`consume`].
pub fn pressed(input: &egui::InputState, action: Action) -> bool {
    action.keys().iter().any(|chord| {
        input.key_pressed(chord.logical_key) && input.modifiers.matches_logically(chord.modifiers)
    })
}

/// Whether each of `action`'s keys was pressed this frame, in the order
/// of [`Action::keys`], taking one press of each. For an action whose keys
/// each mean something of their own, such as Up and Down.
pub fn consume_each(input: &mut egui::InputState, action: Action) -> Vec<bool> {
    action
        .keys()
        .iter()
        .map(|chord| input.consume_shortcut(chord))
        .collect()
}

/// The action whose palette command is `key` (a command's debug name).
pub fn for_command(key: &str) -> Option<Action> {
    // Built once: the palette asks for every command each time it lists them.
    static BY_COMMAND: std::sync::OnceLock<std::collections::HashMap<String, Action>> =
        std::sync::OnceLock::new();
    BY_COMMAND
        .get_or_init(|| {
            Action::all()
                .into_iter()
                .filter_map(|action| Some((action.command()?, action)))
                .collect()
        })
        .get(key)
        .copied()
}

fn modifier_count(modifiers: Modifiers) -> u8 {
    u8::from(modifiers.alt)
        + u8::from(modifiers.shift)
        + u8::from(modifiers.command || modifiers.ctrl || modifiers.mac_cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: Key, modifiers: Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// Runs one frame with `events` and returns what `read` got from it.
    fn with_input<T>(
        events: Vec<egui::Event>,
        modifiers: Modifiers,
        read: impl FnOnce(&mut egui::InputState) -> T,
    ) -> T {
        let ctx = egui::Context::default();
        let mut out = None;
        let mut read = Some(read);
        let _ = crate::theme::run(
            &ctx,
            egui::RawInput {
                events: std::iter::once(egui::Event::ModifiersChanged(modifiers))
                    .chain(events)
                    .collect(),
                ..Default::default()
            },
            |ui| {
                if let Some(read) = read.take() {
                    out = Some(ui.input_mut(read));
                }
            },
        );
        out.unwrap()
    }

    #[test]
    fn no_two_actions_that_can_be_live_together_share_keys() {
        let all = Action::all();
        let mut clashes = Vec::new();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                let (here, there) = (a.context(), b.context());
                if !here.overlaps(there) || here.wins_over(there) || there.wins_over(here) {
                    continue;
                }
                for chord in a.keys() {
                    if b.keys().contains(chord) {
                        clashes.push(format!("{a:?} and {b:?}: {chord:?}"));
                    }
                }
            }
        }
        assert!(clashes.is_empty(), "{clashes:#?}");
    }

    #[test]
    fn every_action_has_keys_or_a_gesture_and_a_label() {
        for action in Action::all() {
            assert!(
                !action.keys().is_empty() || action.gesture().is_some(),
                "{action:?}"
            );
            assert!(!action.label().is_empty());
        }
        let all = Action::all();
        for (i, action) in all.iter().enumerate() {
            assert!(!all[i + 1..].contains(action), "{action:?} listed twice");
        }
        for context in Context::ALL {
            assert!(
                all.iter().any(|action| action.context() == context),
                "{context:?} has shortcuts"
            );
        }
    }

    #[test]
    fn the_tools_keys_come_from_the_table() {
        assert_eq!(Tool::Move.key(), Some(Key::Num2));
        assert_eq!(Tool::Angular.key(), Some(Key::R));
        assert_eq!(TerrainTool::Erode.key(), Some(Key::Num0));
        assert_eq!(TerrainTool::Masks.key(), None);
    }

    #[test]
    fn keys_with_more_modifiers_are_taken_first() {
        // Ctrl+Shift+Z is Redo even though Undo's Ctrl+Z is asked first.
        let got = with_input(vec![press(Key::Z, CMD_SHIFT)], CMD_SHIFT, |input| {
            consume_first(input, &[Action::Undo, Action::Redo])
        });
        assert_eq!(got, Some(Action::Redo));
        let got = with_input(vec![press(Key::F5, SHIFT)], SHIFT, |input| {
            consume_first(input, &[Action::Play, Action::Stop])
        });
        assert_eq!(got, Some(Action::Stop));
        let got = with_input(vec![press(Key::F5, NONE)], NONE, |input| {
            (consume(input, Action::Stop), consume(input, Action::Play))
        });
        assert_eq!(got, (false, true));
        // Taken: asking again finds nothing.
        let got = with_input(vec![press(Key::S, CMD)], CMD, |input| {
            (consume(input, Action::Save), consume(input, Action::Save))
        });
        assert_eq!(got, (true, false));
    }

    #[test]
    fn pressed_leaves_the_key_and_ignores_a_held_shift() {
        let got = with_input(vec![press(Key::Delete, SHIFT)], SHIFT, |input| {
            (
                pressed(input, Action::DeleteNodes),
                pressed(input, Action::DeleteNodes),
                pressed(input, Action::DuplicateNodes),
            )
        });
        assert_eq!(got, (true, true, false));
        let got = with_input(vec![press(Key::Delete, CMD)], CMD, |input| {
            pressed(input, Action::DeleteNodes)
        });
        assert!(!got, "Ctrl isn't ignored");
    }

    #[test]
    fn keys_read_as_settings_and_menus_write_them() {
        let ctx = egui::Context::default();
        ctx.set_os(egui::os::OperatingSystem::Windows);
        assert_eq!(Action::Save.text(&ctx), "Ctrl+S");
        assert_eq!(Action::Redo.text(&ctx), "Ctrl+Y, Ctrl+Shift+Z");
        assert_eq!(Action::Redo.menu_text(&ctx), "Ctrl+Y");
        assert_eq!(Action::ExplorerMove.text(&ctx), "Up, Down");
        assert_eq!(Action::NudgeTen.text(&ctx), "Shift+Left, Right, Up, Down");
        assert_eq!(Action::Delete.menu_text(&ctx), "Del");
        assert_eq!(Action::Rename.menu_text(&ctx), "F2");
        assert_eq!(Action::LookAround.text(&ctx), "Right-drag");
        assert_eq!(Action::BrushBigger.text(&ctx), "]");
        assert_eq!(for_command("Undo"), Some(Action::Undo));
        assert_eq!(for_command("Tool(Move)"), Some(Action::Tool(Tool::Move)));
        assert_eq!(for_command("NewProject"), None);
    }
}
