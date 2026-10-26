use super::*;

/// A function or method as the definitions declare it.
pub(super) struct Function {
    pub(super) name: &'static str,
    pub(super) doc: &'static str,
    /// `(name, type)` pairs. A name ending in `?` may be left out.
    pub(super) params: &'static [(&'static str, &'static str)],
    pub(super) returns: &'static str,
}

pub(super) struct Field {
    pub(super) name: &'static str,
    pub(super) ty: &'static str,
    pub(super) doc: &'static str,
}

/// A type of value that isn't an Instance, such as `Vector3`, or an engine
/// object that isn't one, such as `UserInputService`.
pub(super) struct ValueType {
    pub(super) class: &'static str,
    pub(super) doc: &'static str,
    pub(super) fields: &'static [Field],
    /// Metamethods, declared as methods, such as `__add(self, other: Vector3): Vector3`.
    pub(super) operators: &'static [&'static str],
    pub(super) methods: &'static [Function],
}

/// A global table of functions, such as `Vector3` with `Vector3.new`.
pub(super) struct Library {
    pub(super) global: &'static str,
    pub(super) doc: &'static str,
    pub(super) functions: &'static [Function],
}

pub(super) const VALUE_TYPES: &[ValueType] = &[
    ValueType {
        class: "Vector3",
        doc: "A position, size or direction in studs. Y is up.",
        fields: &[
            Field {
                name: "X",
                ty: "number",
                doc: "The X component.",
            },
            Field {
                name: "Y",
                ty: "number",
                doc: "The Y component (up).",
            },
            Field {
                name: "Z",
                ty: "number",
                doc: "The Z component.",
            },
            Field {
                name: "Magnitude",
                ty: "number",
                doc: "The length of the vector.",
            },
            Field {
                name: "Unit",
                ty: "Vector3",
                doc: "The same direction with length 1 (the zero vector stays zero).",
            },
        ],
        operators: &[
            "__add(self, other: Vector3): Vector3",
            "__sub(self, other: Vector3): Vector3",
            "__mul(self, other: Vector3 | number): Vector3",
            "__div(self, other: Vector3 | number): Vector3",
            "__unm(self): Vector3",
        ],
        methods: &[],
    },
    ValueType {
        class: "Color3",
        doc: "A color, with each channel from 0 to 1.",
        fields: &[
            Field {
                name: "R",
                ty: "number",
                doc: "Red, from 0 to 1.",
            },
            Field {
                name: "G",
                ty: "number",
                doc: "Green, from 0 to 1.",
            },
            Field {
                name: "B",
                ty: "number",
                doc: "Blue, from 0 to 1.",
            },
        ],
        operators: &[],
        methods: &[],
    },
    ValueType {
        class: "UDim",
        doc: "One UI axis as a scale and pixel offset.",
        fields: &[
            Field {
                name: "Scale",
                ty: "number",
                doc: "The fraction of the parent size.",
            },
            Field {
                name: "Offset",
                ty: "number",
                doc: "The pixel offset.",
            },
        ],
        operators: &[],
        methods: &[],
    },
    ValueType {
        class: "UDim2",
        doc: "A UI position or size with scale and pixel-offset components.",
        fields: &[
            Field {
                name: "X",
                ty: "UDim",
                doc: "The horizontal component.",
            },
            Field {
                name: "Y",
                ty: "UDim",
                doc: "The vertical component.",
            },
        ],
        operators: &[
            "__add(self, other: UDim2): UDim2",
            "__sub(self, other: UDim2): UDim2",
        ],
        methods: &[],
    },
    ValueType {
        class: "EnumItem",
        doc: "One choice of an enum, such as `Enum.Material.Neon`.",
        fields: &[Field {
            name: "Name",
            ty: "string",
            doc: "The item's name, such as `\"Neon\"`.",
        }],
        operators: &[],
        methods: &[],
    },
    ValueType {
        class: "UserInputService",
        doc: "The player's keyboard, from `game:GetService(\"UserInputService\")`. LocalScripts only.",
        fields: &[
            Field {
                name: "InputBegan",
                ty: "Signal",
                doc: "Fires when a key goes down. The handler gets an InputObject and `gameProcessedEvent` (always false for now): `function(input, gameProcessedEvent) ... end`.",
            },
            Field {
                name: "InputEnded",
                ty: "Signal",
                doc: "Fires when a key comes back up, with the same arguments as InputBegan.",
            },
        ],
        operators: &[],
        methods: &[Function {
            name: "IsKeyDown",
            doc: "True while `key` is held down, such as `Enum.KeyCode.W`.",
            params: &[("key", "EnumKeyCode")],
            returns: "boolean",
        }],
    },
    ValueType {
        class: "RunService",
        doc: "The game's frame loop, from `game:GetService(\"RunService\")`.",
        fields: &[Field {
            name: "Heartbeat",
            ty: "Signal",
            doc: "Fires once every frame, after physics. The handler gets the frame's length in seconds: `function(deltaTime) ... end`.",
        }],
        operators: &[],
        methods: &[],
    },
    ValueType {
        class: "ReplicatedFirst",
        doc: "What loads first on the player's side, from `game:GetService(\"ReplicatedFirst\")`.",
        fields: &[],
        operators: &[],
        methods: &[Function {
            name: "RemoveDefaultLoadingScreen",
            doc: "Says the game is ready: the loading screen goes once it has been up for its minimum time. Only waited for when the project's loading screen is set to wait for a script. LocalScripts only.",
            params: &[],
            returns: "()",
        }],
    },
    ValueType {
        class: "TweenService",
        doc: "Moves properties smoothly to goals over time, from `game:GetService(\"TweenService\")`.",
        fields: &[],
        operators: &[],
        methods: &[Function {
            name: "Create",
            doc: "A Tween that moves each property in `goals` (a table such as `{Position = Vector3.new(0, 5, 0)}`) of `instance` from where it is to its goal, as `tweenInfo` says. Numbers, Vector3s, Color3s, UDim2s and booleans (which switch at the end) can be tweened. Call `Play` to start it.",
            params: &[
                ("instance", "Instance"),
                ("tweenInfo", "TweenInfo"),
                ("goals", "{ [string]: any }"),
            ],
            returns: "Tween",
        }],
    },
    ValueType {
        class: "Tween",
        doc: "Properties moving to goals, made by `TweenService:Create`. Each frame it sets them, so their change events fire; a newer tween on the same property of the same Instance cancels it.",
        fields: &[
            Field {
                name: "PlaybackState",
                ty: "EnumPlaybackState",
                doc: "Where it is: Begin, Delayed, Playing, Paused, Completed or Cancelled (read-only).",
            },
            Field {
                name: "Completed",
                ty: "Signal",
                doc: "Fires when it reaches its goals, or is cancelled, with how it ended: `function(playbackState) ... end`.",
            },
            Field {
                name: "Instance",
                ty: "Instance",
                doc: "The Instance it moves (read-only).",
            },
            Field {
                name: "TweenInfo",
                ty: "TweenInfo",
                doc: "How it plays (read-only).",
            },
        ],
        operators: &[],
        methods: &[
            Function {
                name: "Play",
                doc: "Starts it from where the properties are now (after its DelayTime), or carries on after Pause.",
                params: &[],
                returns: "()",
            },
            Function {
                name: "Pause",
                doc: "Holds it where it is; Play carries on from there.",
                params: &[],
                returns: "()",
            },
            Function {
                name: "Cancel",
                doc: "Stops it where it is, and fires Completed with `Enum.PlaybackState.Cancelled`. Playing it again starts over from there.",
                params: &[],
                returns: "()",
            },
        ],
    },
    ValueType {
        class: "TweenInfo",
        doc: "How a tween plays, from `TweenInfo.new`. Read-only.",
        fields: &[
            Field {
                name: "Time",
                ty: "number",
                doc: "Seconds to reach the goals.",
            },
            Field {
                name: "EasingStyle",
                ty: "EnumEasingStyle",
                doc: "The shape of the motion: Linear, or one of the clip key easings (Sine, Quad, Cubic, Back, Bounce, Elastic).",
            },
            Field {
                name: "EasingDirection",
                ty: "EnumEasingDirection",
                doc: "Which end of the motion is eased: In (the start), Out (the end) or InOut (both).",
            },
            Field {
                name: "RepeatCount",
                ty: "number",
                doc: "How many more times it plays after the first; -1 repeats forever.",
            },
            Field {
                name: "Reverses",
                ty: "boolean",
                doc: "Whether each play goes back to the start after reaching the goals.",
            },
            Field {
                name: "DelayTime",
                ty: "number",
                doc: "Seconds to wait after Play before it starts.",
            },
        ],
        operators: &[],
        methods: &[],
    },
    ValueType {
        class: "InputObject",
        doc: "One key going down or up, as `InputBegan` and `InputEnded` handlers get it.",
        fields: &[Field {
            name: "KeyCode",
            ty: "EnumKeyCode",
            doc: "Which key, such as `Enum.KeyCode.W`.",
        }],
        operators: &[],
        methods: &[],
    },
    ValueType {
        class: "RaycastResult",
        doc: "Where `workspace:Raycast` first met a collider, or nil when it missed.",
        fields: &[
            Field {
                name: "Instance",
                ty: "Instance",
                doc: "The Part, MeshPart or Terrain the ray hit.",
            },
            Field {
                name: "Position",
                ty: "Vector3",
                doc: "The point of contact, in studs.",
            },
            Field {
                name: "Normal",
                ty: "Vector3",
                doc: "The surface direction at the hit, pointing out.",
            },
            Field {
                name: "Distance",
                ty: "number",
                doc: "How far along the ray the hit is, in studs.",
            },
        ],
        operators: &[],
        methods: &[],
    },
    ValueType {
        class: "Camera",
        doc: "What the player sees through, `workspace.CurrentCamera`. LocalScripts only. Setting Position or Orientation moves the view; the player can still fly on from there.",
        fields: &[
            Field {
                name: "Position",
                ty: "Vector3",
                doc: "Where the camera is, in studs.",
            },
            Field {
                name: "Orientation",
                ty: "Vector3",
                doc: "Which way it looks, in degrees like a Part's: X looks up (positive) and down, Y turns left (positive) and right. Z is ignored.",
            },
            Field {
                name: "LookVector",
                ty: "Vector3",
                doc: "The direction the camera looks in, one stud long. Read-only.",
            },
        ],
        operators: &[],
        methods: &[],
    },
];

/// Fields every Instance has. Properties come from `core` instead.
pub(super) const INSTANCE_FIELDS: &[Field] = &[
    Field {
        name: "Name",
        ty: "string",
        doc: "The Instance's name. Children can be found by name: `workspace.Block`.",
    },
    Field {
        name: "ClassName",
        ty: "string",
        doc: "The Instance's class, such as `\"Part\"`. Read-only.",
    },
    Field {
        name: "Parent",
        ty: "Instance?",
        doc: "The Instance this one is inside, or nil. Setting it moves the Instance.",
    },
];

pub(super) const INSTANCE_METHODS: &[Function] = &[
    Function {
        name: "FindFirstChild",
        doc: "The first child with this name, or nil. With `recursive`, searches every descendant.",
        params: &[("name", "string"), ("recursive?", "boolean")],
        returns: "Instance?",
    },
    Function {
        name: "GetPropertyChangedSignal",
        doc: "An event that fires when this one property (or `Name` or `Parent`) gets a different value. The handler gets nothing: read the property to see its new value.",
        params: &[("property", "string")],
        returns: "Signal",
    },
];

/// What each event every Instance has means (`core`'s `INSTANCE_EVENTS`).
/// A test checks they match.
pub(super) const INSTANCE_EVENT_DOCS: &[(&str, &str)] = &[
    (
        "Changed",
        "Fires when a property (or `Name` or `Parent`) gets a different value, whether a script, physics or the editor changed it. The handler gets the property's name: `function(property) ... end`.",
    ),
    (
        "ChildAdded",
        "Fires when an Instance is put inside this one. The handler gets the child.",
    ),
    (
        "ChildRemoved",
        "Fires when an Instance is taken out of this one. The handler gets the child.",
    ),
];

/// What `part.Touched` and other events are.
pub(super) const SIGNAL_DOC: &str =
    "An event, such as `part.Touched`. Connect a function to run it every time the event fires.";

pub(super) const SIGNAL_METHODS: &[Function] = &[Function {
    name: "Connect",
    doc: "Runs `callback` every time the event fires, each time as its own task (so it may `task.wait`). Returns a Connection for stopping it.",
    params: &[("callback", "(...any) -> ()")],
    returns: "Connection",
}];

pub(super) const CONNECTION_DOC: &str = "A function connected to an event, returned by `Connect`.";

pub(super) const CONNECTION_FIELDS: &[Field] = &[Field {
    name: "Connected",
    ty: "boolean",
    doc: "True until `Disconnect` is called (or the Instance is destroyed).",
}];

pub(super) const CONNECTION_METHODS: &[Function] = &[Function {
    name: "Disconnect",
    doc: "Stops the function from running when the event fires again. Calling it twice is harmless.",
    params: &[],
    returns: "()",
}];

/// Globals that are Instances: name, Luau type, description.
pub(super) const INSTANCE_GLOBALS: &[(&str, &str, &str)] = &[
    (
        "game",
        "DataModel",
        "The root of the game. Every service is inside it.",
    ),
    (
        "workspace",
        "Workspace",
        "The service that holds the 3D world (same as `game.Workspace`).",
    ),
    (
        "script",
        "Script | LocalScript | ModuleScript",
        "The Script, LocalScript or ModuleScript that is running this code.",
    ),
];

/// Global functions the engine adds or replaces.
pub(super) const GLOBAL_FUNCTIONS: &[Function] = &[Function {
    name: "require",
    doc: "Runs a ModuleScript (a `.module.lua` file, such as `require(game.ReplicatedStorage.Utils)`) the first time, and returns what it returned. Later calls on the same side return the same value.",
    // `Instance`, not `ModuleScript`: a child found by name could be any
    // class (see the Instance indexer), and `require` checks it when it runs.
    params: &[("module", "Instance")],
    returns: "any",
}];

/// Methods only some classes have.
pub(super) const CLASS_METHODS: &[(ClassName, Function)] = &[
    (
        ClassName::DataModel,
        Function {
            name: "GetService",
            doc: "The service with this class name, such as `\"Players\"`, `\"ReplicatedFirst\"`, `\"RunService\"`, `\"TweenService\"` or `\"UserInputService\"`.",
            params: &[("className", "string")],
            returns: "Instance",
        },
    ),
    (
        ClassName::RemoteEvent,
        Function {
            name: "FireServer",
            doc: "Sends the arguments to the server, where `OnServerEvent` fires with this player first.",
            params: &[("...", "any")],
            returns: "()",
        },
    ),
    (
        ClassName::RemoteEvent,
        Function {
            name: "FireClient",
            doc: "Sends the arguments to one player, where `OnClientEvent` fires.",
            params: &[("player", "Player"), ("...", "any")],
            returns: "()",
        },
    ),
    (
        ClassName::RemoteEvent,
        Function {
            name: "FireAllClients",
            doc: "Sends the arguments to every player, where `OnClientEvent` fires.",
            params: &[("...", "any")],
            returns: "()",
        },
    ),
    (
        ClassName::Workspace,
        Function {
            name: "Raycast",
            doc: "The first collider along `direction` from `origin`, or nil. The direction's length is how far the ray reaches. Hits sensors (`CanCollide` off) as well as solid parts and terrain. The result has Instance, Position, Normal and Distance.",
            params: &[("origin", "Vector3"), ("direction", "Vector3")],
            returns: "RaycastResult?",
        },
    ),
    (
        ClassName::Sound,
        Function {
            name: "Play",
            doc: "Plays this sound from the start, with its SoundId, Volume, PlaybackSpeed, Looped and Spatial, and sets Playing to true. Playing it again restarts it; stopping the game session stops all sounds.",
            params: &[],
            returns: "()",
        },
    ),
    (
        ClassName::Sound,
        Function {
            name: "Stop",
            doc: "Stops this sound and sets Playing to false. The next Play starts from the beginning.",
            params: &[],
            returns: "()",
        },
    ),
    (
        ClassName::Sound,
        Function {
            name: "Pause",
            doc: "Pauses this sound where it is (sets Playing to false). Resume carries on from there.",
            params: &[],
            returns: "()",
        },
    ),
    (
        ClassName::Animator,
        Function {
            name: "SetBool",
            doc: "Sets a Bool parameter, such as `IsGrounded`: something that stays true or false until it's changed.",
            params: &[("name", "string"), ("value", "boolean")],
            returns: "()",
        },
    ),
    (
        ClassName::Animator,
        Function {
            name: "GetBool",
            doc: "A Bool parameter's value.",
            params: &[("name", "string")],
            returns: "boolean",
        },
    ),
    (
        ClassName::Animator,
        Function {
            name: "SetNumber",
            doc: "Sets a Number parameter, such as `Speed`: a value that changes gradually, for conditions and blend trees.",
            params: &[("name", "string"), ("value", "number")],
            returns: "()",
        },
    ),
    (
        ClassName::Animator,
        Function {
            name: "GetNumber",
            doc: "A Number parameter's value.",
            params: &[("name", "string")],
            returns: "number",
        },
    ),
    (
        ClassName::Animator,
        Function {
            name: "SetTrigger",
            doc: "Sets a Trigger parameter, such as `Jump`: a one-time request, used up by the transition it starts.",
            params: &[("name", "string")],
            returns: "()",
        },
    ),
    (
        ClassName::Animator,
        Function {
            name: "ResetTrigger",
            doc: "Clears a Trigger that no transition has used yet.",
            params: &[("name", "string")],
            returns: "()",
        },
    ),
    (
        ClassName::Animator,
        Function {
            name: "Play",
            doc: "Switches to the state named `state`, from its start, blending from the pose now over `blend` seconds (0.25 if left out; 0 snaps).",
            params: &[("state", "string"), ("blend?", "number")],
            returns: "()",
        },
    ),
    (
        ClassName::Animator,
        Function {
            name: "LoadAnimation",
            doc: "Loads `animation` (an Animation, whose Clip names the clip) on this Animator and returns an AnimationTrack to play, stop and blend over the state machine.",
            params: &[("animation", "Animation")],
            returns: "AnimationTrack",
        },
    ),
    (
        ClassName::Animator,
        Function {
            name: "GetPlayingAnimationTracks",
            doc: "The AnimationTracks loaded on this Animator that are playing now.",
            params: &[],
            returns: "{AnimationTrack}",
        },
    ),
    (
        ClassName::AnimationTrack,
        Function {
            name: "Play",
            doc: "Plays the clip from its start, fading in over `fadeTime` seconds (0.1 if left out) to `weight` (1), at `speed` (1). Where it and what plays under it animate the same thing, the track started last wins.",
            params: &[
                ("fadeTime?", "number"),
                ("weight?", "number"),
                ("speed?", "number"),
            ],
            returns: "()",
        },
    ),
    (
        ClassName::AnimationTrack,
        Function {
            name: "Stop",
            doc: "Stops it, fading out over `fadeTime` seconds (0.1 if left out). Stopped fires once it has faded out.",
            params: &[("fadeTime?", "number")],
            returns: "()",
        },
    ),
    (
        ClassName::AnimationTrack,
        Function {
            name: "AdjustSpeed",
            doc: "Plays it this many times as fast from now (1 is normal, 0 holds, negative plays backwards).",
            params: &[("speed?", "number")],
            returns: "()",
        },
    ),
    (
        ClassName::AnimationTrack,
        Function {
            name: "AdjustWeight",
            doc: "Fades how much it counts to `weight` (0 to 1) over `fadeTime` seconds (0.1 if left out).",
            params: &[("weight?", "number"), ("fadeTime?", "number")],
            returns: "()",
        },
    ),
    (
        ClassName::AnimationTrack,
        Function {
            name: "GetMarkerReachedSignal",
            doc: "An event that fires each time the track passes the clip's event named `name`. The handler gets the name.",
            params: &[("name", "string")],
            returns: "Signal",
        },
    ),
    (
        ClassName::Animation,
        Function {
            name: "Play",
            doc: "Plays this Animation's clip from the start, through the Animator on the same parent. `animator:LoadAnimation(animation)` is the newer way, with fades and weights.",
            params: &[],
            returns: "()",
        },
    ),
    (
        ClassName::Animation,
        Function {
            name: "Stop",
            doc: "Stops playback and goes back to the start of the clip.",
            params: &[],
            returns: "()",
        },
    ),
    (
        ClassName::Animation,
        Function {
            name: "Pause",
            doc: "Pauses at the current time. Resume continues from there.",
            params: &[],
            returns: "()",
        },
    ),
    (
        ClassName::Animation,
        Function {
            name: "Resume",
            doc: "Continues after Pause.",
            params: &[],
            returns: "()",
        },
    ),
    (
        ClassName::ParticleEmitter,
        Function {
            name: "Emit",
            doc: "Gives off this many particles at once (up to 1000), even while Enabled is false, such as a burst of sparks when something breaks.",
            params: &[("count", "number")],
            returns: "()",
        },
    ),
    (
        ClassName::Sound,
        Function {
            name: "Resume",
            doc: "Carries on playing a paused sound, or plays a stopped one from the start (sets Playing to true).",
            params: &[],
            returns: "()",
        },
    ),
];

/// Fields only some classes have, besides their properties and events.
pub(super) const CLASS_FIELDS: &[(ClassName, Field)] = &[
    (
        ClassName::DataModel,
        Field {
            name: "UserInputService",
            ty: "UserInputService",
            doc: "The player's keyboard (same as `game:GetService(\"UserInputService\")`).",
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "RunService",
            ty: "RunService",
            doc: "The game's frame loop, with `Heartbeat` (same as `game:GetService(\"RunService\")`).",
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "TweenService",
            ty: "TweenService",
            doc: "Moves properties smoothly to goals (same as `game:GetService(\"TweenService\")`).",
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "ReplicatedFirst",
            ty: "ReplicatedFirst",
            doc: "What loads first on the player's side, with `RemoveDefaultLoadingScreen` (same as `game:GetService(\"ReplicatedFirst\")`).",
        },
    ),
    (
        ClassName::Workspace,
        Field {
            name: "CurrentCamera",
            ty: "Camera",
            doc: "The camera the player sees through.",
        },
    ),
    (
        ClassName::Players,
        Field {
            name: "LocalPlayer",
            ty: "Player",
            doc: "The player on this device.",
        },
    ),
    (
        ClassName::Animator,
        Field {
            name: "CurrentState",
            ty: "string",
            doc: "The name of the state playing (read-only; use `Play` to change it). Empty until the animator first runs.",
        },
    ),
    (
        ClassName::AnimationTrack,
        Field {
            name: "IsPlaying",
            ty: "boolean",
            doc: "True from Play until Stop, or until a clip that doesn't loop ends (read-only).",
        },
    ),
    (
        ClassName::AnimationTrack,
        Field {
            name: "Length",
            ty: "number",
            doc: "The clip's length in seconds (read-only; 0 until the clip has been read, a frame after LoadAnimation).",
        },
    ),
    (
        ClassName::AnimationTrack,
        Field {
            name: "Looped",
            ty: "boolean",
            doc: "Whether it plays again from the start when it ends. Starts as the clip's own.",
        },
    ),
    (
        ClassName::AnimationTrack,
        Field {
            name: "Speed",
            ty: "number",
            doc: "How fast it plays (read-only; use AdjustSpeed).",
        },
    ),
    (
        ClassName::AnimationTrack,
        Field {
            name: "TimePosition",
            ty: "number",
            doc: "Seconds into the clip. Set it to jump there.",
        },
    ),
    (
        ClassName::AnimationTrack,
        Field {
            name: "WeightCurrent",
            ty: "number",
            doc: "How much it counts now, from 0 to 1, while fading (read-only).",
        },
    ),
    (
        ClassName::AnimationTrack,
        Field {
            name: "WeightTarget",
            ty: "number",
            doc: "The weight it's fading to (read-only; use AdjustWeight).",
        },
    ),
    (
        ClassName::AnimationTrack,
        Field {
            name: "Animation",
            ty: "Animation",
            doc: "The Animation it was loaded from (read-only).",
        },
    ),
];

pub(super) const LIBRARIES: &[Library] = &[
    Library {
        global: "Instance",
        doc: "Creates Instances.",
        functions: &[Function {
            name: "new",
            doc: "Creates an Instance of this class with default properties and no parent. Set `Parent` to put it in the world.",
            params: &[("className", "string")],
            returns: "Instance",
        }],
    },
    Library {
        global: "Vector3",
        doc: "Creates Vector3 values.",
        functions: &[Function {
            name: "new",
            doc: "A new Vector3. Left-out components are 0.",
            params: &[("x?", "number"), ("y?", "number"), ("z?", "number")],
            returns: "Vector3",
        }],
    },
    Library {
        global: "Color3",
        doc: "Creates Color3 values.",
        functions: &[
            Function {
                name: "new",
                doc: "A color from channels between 0 and 1. Left-out channels are 0.",
                params: &[("r?", "number"), ("g?", "number"), ("b?", "number")],
                returns: "Color3",
            },
            Function {
                name: "fromRGB",
                doc: "A color from channels between 0 and 255, as in most paint programs.",
                params: &[("r?", "number"), ("g?", "number"), ("b?", "number")],
                returns: "Color3",
            },
        ],
    },
    Library {
        global: "UDim2",
        doc: "Creates UI positions and sizes.",
        functions: &[
            Function {
                name: "new",
                doc: "A UI value from scale and pixel offset on each axis. Left-out components are 0.",
                params: &[
                    ("xScale?", "number"),
                    ("xOffset?", "number"),
                    ("yScale?", "number"),
                    ("yOffset?", "number"),
                ],
                returns: "UDim2",
            },
            Function {
                name: "fromScale",
                doc: "A UI value made from scale components, with no pixel offsets.",
                params: &[("x?", "number"), ("y?", "number")],
                returns: "UDim2",
            },
            Function {
                name: "fromOffset",
                doc: "A UI value made from pixel offsets, with no scale components.",
                params: &[("x?", "number"), ("y?", "number")],
                returns: "UDim2",
            },
        ],
    },
    Library {
        global: "TweenInfo",
        doc: "Creates TweenInfo values.",
        functions: &[Function {
            name: "new",
            doc: "How a tween plays. Left out: 1 second, `Enum.EasingStyle.Quad`, `Enum.EasingDirection.Out`, no repeats, no reversing and no delay. The style and direction can also be item names, such as `\"Sine\"`.",
            params: &[
                ("time?", "number"),
                ("easingStyle?", "EnumEasingStyle"),
                ("easingDirection?", "EnumEasingDirection"),
                ("repeatCount?", "number"),
                ("reverses?", "boolean"),
                ("delayTime?", "number"),
            ],
            returns: "TweenInfo",
        }],
    },
    Library {
        global: "task",
        doc: "Pauses scripts and runs functions later, without stopping the game.",
        functions: &[
            Function {
                name: "wait",
                doc: "Pauses this code for `seconds` (left out: until the next frame), while the game and other scripts keep running. Returns the seconds that really passed.",
                params: &[("seconds?", "number")],
                returns: "number",
            },
            Function {
                name: "spawn",
                doc: "Runs `callback` right away with the given arguments, until it finishes or waits, then carries on here. An error inside it is reported without stopping this code.",
                params: &[("callback", "(...any) -> ()"), ("...", "any")],
                returns: "thread",
            },
            Function {
                name: "delay",
                doc: "Runs `callback` with the given arguments after `seconds`, without pausing this code.",
                params: &[
                    ("seconds", "number"),
                    ("callback", "(...any) -> ()"),
                    ("...", "any"),
                ],
                returns: "thread",
            },
        ],
    },
];

pub(super) const ENUM_GLOBAL_DOC: &str =
    "Every enum, such as `Enum.Material`, `Enum.PartType` and `Enum.KeyCode`.";
