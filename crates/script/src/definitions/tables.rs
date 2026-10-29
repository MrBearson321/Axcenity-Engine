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
    /// Values in the table, such as `Vector3.zero`.
    pub(super) constants: &'static [Field],
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
        methods: &[
            Function {
                name: "Dot",
                doc: "The dot product: the lengths multiplied, times the cosine of the angle between them.",
                params: &[("other", "Vector3")],
                returns: "number",
            },
            Function {
                name: "Cross",
                doc: "A vector at right angles to both, as long as the area they span.",
                params: &[("other", "Vector3")],
                returns: "Vector3",
            },
            Function {
                name: "Lerp",
                doc: "The point `alpha` of the way from this vector to `goal` (0 is this one, 1 is `goal`).",
                params: &[("goal", "Vector3"), ("alpha", "number")],
                returns: "Vector3",
            },
            Function {
                name: "Angle",
                doc: "The angle to `other` in radians, from 0 to pi. With an `axis`, it's negative when turning to `other` goes clockwise looking down the axis.",
                params: &[("other", "Vector3"), ("axis?", "Vector3")],
                returns: "number",
            },
            Function {
                name: "FuzzyEq",
                doc: "Whether each component is within `epsilon` (left out: 0.00001) of `other`'s.",
                params: &[("other", "Vector3"), ("epsilon?", "number")],
                returns: "boolean",
            },
            Function {
                name: "Min",
                doc: "The smallest of each component of this and the other vectors.",
                params: &[("...", "Vector3")],
                returns: "Vector3",
            },
            Function {
                name: "Max",
                doc: "The largest of each component of this and the other vectors.",
                params: &[("...", "Vector3")],
                returns: "Vector3",
            },
            Function {
                name: "Abs",
                doc: "Each component without its sign.",
                params: &[],
                returns: "Vector3",
            },
            Function {
                name: "Floor",
                doc: "Each component rounded down.",
                params: &[],
                returns: "Vector3",
            },
            Function {
                name: "Ceil",
                doc: "Each component rounded up.",
                params: &[],
                returns: "Vector3",
            },
        ],
    },
    ValueType {
        class: "Vector2",
        doc: "A point or size on the screen, in pixels, such as a ScrollingFrame's CanvasPosition.",
        fields: &[
            Field {
                name: "X",
                ty: "number",
                doc: "Across, in pixels.",
            },
            Field {
                name: "Y",
                ty: "number",
                doc: "Down, in pixels.",
            },
            Field {
                name: "Magnitude",
                ty: "number",
                doc: "The length of the vector.",
            },
        ],
        operators: &[
            "__add(self, other: Vector2): Vector2",
            "__sub(self, other: Vector2): Vector2",
            "__mul(self, other: Vector2 | number): Vector2",
            "__div(self, other: Vector2 | number): Vector2",
            "__unm(self): Vector2",
        ],
        methods: &[
            Function {
                name: "Dot",
                doc: "The dot product: the lengths multiplied, times the cosine of the angle between them.",
                params: &[("other", "Vector2")],
                returns: "number",
            },
            Function {
                name: "Lerp",
                doc: "The point `alpha` of the way from this vector to `goal` (0 is this one, 1 is `goal`).",
                params: &[("goal", "Vector2"), ("alpha", "number")],
                returns: "Vector2",
            },
            Function {
                name: "Angle",
                doc: "The angle to `other` in radians, from 0 to pi. Signed, it's negative when `other` is anticlockwise from this one on the screen.",
                params: &[("other", "Vector2"), ("isSigned?", "boolean")],
                returns: "number",
            },
            Function {
                name: "FuzzyEq",
                doc: "Whether each component is within `epsilon` (left out: 0.00001) of `other`'s.",
                params: &[("other", "Vector2"), ("epsilon?", "number")],
                returns: "boolean",
            },
            Function {
                name: "Min",
                doc: "The smallest of each component of this and the other vectors.",
                params: &[("...", "Vector2")],
                returns: "Vector2",
            },
            Function {
                name: "Max",
                doc: "The largest of each component of this and the other vectors.",
                params: &[("...", "Vector2")],
                returns: "Vector2",
            },
            Function {
                name: "Abs",
                doc: "Each component without its sign.",
                params: &[],
                returns: "Vector2",
            },
            Function {
                name: "Floor",
                doc: "Each component rounded down.",
                params: &[],
                returns: "Vector2",
            },
            Function {
                name: "Ceil",
                doc: "Each component rounded up.",
                params: &[],
                returns: "Vector2",
            },
        ],
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
        methods: &[
            Function {
                name: "Lerp",
                doc: "The color `alpha` of the way from this one to `goal` (0 is this one, 1 is `goal`).",
                params: &[("goal", "Color3"), ("alpha", "number")],
                returns: "Color3",
            },
            Function {
                name: "ToHSV",
                doc: "Hue, saturation and value, each from 0 to 1, as `Color3.fromHSV` takes them.",
                params: &[],
                returns: "(number, number, number)",
            },
            Function {
                name: "ToHex",
                doc: "Six lowercase hex digits without a `#`, such as `\"ff8000\"`.",
                params: &[],
                returns: "string",
            },
        ],
    },
    ValueType {
        class: "CFrame",
        doc: "A position and a rotation together. `part.CFrame` reads and writes the part's Position and Orientation; a CFrame isn't saved.",
        fields: &[
            Field {
                name: "Position",
                ty: "Vector3",
                doc: "Where it is.",
            },
            Field {
                name: "X",
                ty: "number",
                doc: "The position's X.",
            },
            Field {
                name: "Y",
                ty: "number",
                doc: "The position's Y.",
            },
            Field {
                name: "Z",
                ty: "number",
                doc: "The position's Z.",
            },
            Field {
                name: "Rotation",
                ty: "CFrame",
                doc: "The same turn at the origin.",
            },
            Field {
                name: "LookVector",
                ty: "Vector3",
                doc: "The way its front faces (its -Z), one stud long.",
            },
            Field {
                name: "RightVector",
                ty: "Vector3",
                doc: "Its right side (its +X), one stud long.",
            },
            Field {
                name: "UpVector",
                ty: "Vector3",
                doc: "Its top (its +Y), one stud long.",
            },
        ],
        operators: &[
            "__mul(self, other: CFrame): CFrame",
            "__mul(self, other: Vector3): Vector3",
            "__add(self, other: Vector3): CFrame",
            "__sub(self, other: Vector3): CFrame",
        ],
        methods: &[
            Function {
                name: "Inverse",
                doc: "The CFrame that undoes this one: `cf * cf:Inverse()` is `CFrame.identity`.",
                params: &[],
                returns: "CFrame",
            },
            Function {
                name: "Lerp",
                doc: "The CFrame `alpha` of the way to `goal`: the position in a straight line, and the rotation the short way round.",
                params: &[("goal", "CFrame"), ("alpha", "number")],
                returns: "CFrame",
            },
            Function {
                name: "ToWorldSpace",
                doc: "`other`, given relative to this CFrame, in world space (`cf * other`).",
                params: &[("other", "CFrame")],
                returns: "CFrame",
            },
            Function {
                name: "ToObjectSpace",
                doc: "`other`, given in world space, relative to this CFrame (`cf:Inverse() * other`).",
                params: &[("other", "CFrame")],
                returns: "CFrame",
            },
            Function {
                name: "PointToWorldSpace",
                doc: "A point given relative to this CFrame, in world space (`cf * point`).",
                params: &[("point", "Vector3")],
                returns: "Vector3",
            },
            Function {
                name: "PointToObjectSpace",
                doc: "A point given in world space, relative to this CFrame.",
                params: &[("point", "Vector3")],
                returns: "Vector3",
            },
            Function {
                name: "VectorToWorldSpace",
                doc: "A direction given relative to this CFrame, turned into world space (the position doesn't count).",
                params: &[("direction", "Vector3")],
                returns: "Vector3",
            },
            Function {
                name: "VectorToObjectSpace",
                doc: "A direction given in world space, turned into this CFrame's space (the position doesn't count).",
                params: &[("direction", "Vector3")],
                returns: "Vector3",
            },
            Function {
                name: "ToEulerAnglesYXZ",
                doc: "The angles around X, Y and Z in radians that `CFrame.fromEulerAnglesYXZ` takes: an `Orientation`, in radians.",
                params: &[],
                returns: "(number, number, number)",
            },
        ],
    },
    ValueType {
        class: "Random",
        doc: "A random number generator from `Random.new(seed)`. The same seed gives the same numbers on every run and platform.",
        fields: &[],
        operators: &[],
        methods: &[
            Function {
                name: "NextNumber",
                doc: "A number from `min` (left out: 0) up to `max` (left out: 1).",
                params: &[("min?", "number"), ("max?", "number")],
                returns: "number",
            },
            Function {
                name: "NextInteger",
                doc: "A whole number from `min` to `max`, both included.",
                params: &[("min", "number"), ("max", "number")],
                returns: "number",
            },
            Function {
                name: "NextUnitVector",
                doc: "A direction one stud long, every direction equally likely.",
                params: &[],
                returns: "Vector3",
            },
            Function {
                name: "Shuffle",
                doc: "Puts the items of a list in a random order, in place.",
                params: &[("list", "{ any }")],
                returns: "()",
            },
            Function {
                name: "Clone",
                doc: "A new generator that gives the same numbers this one would from here.",
                params: &[],
                returns: "Random",
            },
        ],
    },
    ValueType {
        class: "PhysicalProperties",
        doc: "How a part weighs, grips and bounces. Where two parts touch, each coefficient is the average of theirs, weighted by their weights.",
        fields: &[
            Field {
                name: "Density",
                ty: "number",
                doc: "Mass per cubic stud, from 0.0001 to 100. Plastic is 1, Metal 7.85.",
            },
            Field {
                name: "Friction",
                ty: "number",
                doc: "From 0 (slides like ice) to 2 (grips).",
            },
            Field {
                name: "Elasticity",
                ty: "number",
                doc: "How much of its speed a bounce keeps, from 0 (none) to 1 (all).",
            },
            Field {
                name: "FrictionWeight",
                ty: "number",
                doc: "How much its friction counts against the other part's, from 0 to 100.",
            },
            Field {
                name: "ElasticityWeight",
                ty: "number",
                doc: "How much its elasticity counts against the other part's, from 0 to 100.",
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
        fields: &[
            Field {
                name: "Name",
                ty: "string",
                doc: "The item's name, such as `\"Neon\"`.",
            },
            Field {
                name: "Value",
                ty: "number",
                doc: "The item's number: an `Enum.RenderPriority`'s priority (`Enum.RenderPriority.Camera.Value` is 200), and for other enums its place in the list, from 0.",
            },
        ],
        operators: &[],
        methods: &[],
    },
    ValueType {
        class: "UserInputService",
        doc: "The player's keyboard, mouse, touch screen and game controllers, and the input map's actions, from `game:GetService(\"UserInputService\")`. LocalScripts only.",
        fields: &[
            Field {
                name: "InputBegan",
                ty: "Signal",
                doc: "Fires when a key, mouse button, controller button or finger goes down. The handler gets an InputObject and `gameProcessedEvent`, true when the game's UI took it (a click on a button, typing in a TextBox, a key that moved focus): `function(input, gameProcessedEvent) ... end`.",
            },
            Field {
                name: "InputChanged",
                ty: "Signal",
                doc: "Fires when the mouse moves, the wheel turns, a finger moves or a controller's stick moves, with the same arguments as InputBegan. `input.Delta` is how far it moved; for the wheel, `input.Position.Z` is the notches.",
            },
            Field {
                name: "InputEnded",
                ty: "Signal",
                doc: "Fires when a key, button or finger comes back up, with the same arguments as InputBegan.",
            },
            Field {
                name: "TouchStarted",
                ty: "Signal",
                doc: "Fires when a finger touches the screen, with the same arguments as InputBegan (before InputBegan).",
            },
            Field {
                name: "TouchMoved",
                ty: "Signal",
                doc: "Fires when a finger moves on the screen, with the same arguments as InputBegan.",
            },
            Field {
                name: "TouchEnded",
                ty: "Signal",
                doc: "Fires when a finger leaves the screen, with the same arguments as InputBegan.",
            },
            Field {
                name: "GamepadConnected",
                ty: "Signal",
                doc: "Fires when a controller is plugged in (or first used). The handler gets which one: `function(gamepad) ... end`, such as `Enum.UserInputType.Gamepad1`.",
            },
            Field {
                name: "GamepadDisconnected",
                ty: "Signal",
                doc: "Fires when a controller is unplugged, with which one, as GamepadConnected.",
            },
            Field {
                name: "MouseBehavior",
                ty: "EnumMouseBehavior",
                doc: "What the cursor does: `Default` moves freely; `LockCenter` holds it, hidden, in the middle of the window, and `LockCurrentPosition` where it is. While it's held, `GetMouseDelta` still says how far the mouse moved, for a camera.",
            },
            Field {
                name: "MouseIconEnabled",
                ty: "boolean",
                doc: "False hides the cursor over the game.",
            },
        ],
        operators: &[],
        methods: &[
            Function {
                name: "IsKeyDown",
                doc: "True while keyboard key `key` is held down, such as `Enum.KeyCode.W`.",
                params: &[("key", "EnumKeyCode")],
                returns: "boolean",
            },
            Function {
                name: "IsMouseButtonPressed",
                doc: "True while mouse button `button` is held, such as `Enum.UserInputType.MouseButton1`.",
                params: &[("button", "EnumUserInputType")],
                returns: "boolean",
            },
            Function {
                name: "GetMouseLocation",
                doc: "Where the mouse is, in pixels from the window's top left.",
                params: &[],
                returns: "Vector2",
            },
            Function {
                name: "GetMouseDelta",
                doc: "How far the mouse moved this frame, in pixels, right and down positive. It works while the cursor is locked (`MouseBehavior`).",
                params: &[],
                returns: "Vector2",
            },
            Function {
                name: "GetConnectedGamepads",
                doc: "The controllers plugged in, such as `{Enum.UserInputType.Gamepad1}`.",
                params: &[],
                returns: "{EnumUserInputType}",
            },
            Function {
                name: "IsGamepadButtonDown",
                doc: "True while controller `gamepad` holds `button`, such as `Enum.KeyCode.ButtonA`.",
                params: &[("gamepad", "EnumUserInputType"), ("button", "EnumKeyCode")],
                returns: "boolean",
            },
            Function {
                name: "GetGamepadState",
                doc: "An InputObject for each of controller `gamepad`'s buttons and sticks: `UserInputState` Begin while held, and the sticks' `Position`.",
                params: &[("gamepad", "EnumUserInputType")],
                returns: "{InputObject}",
            },
            Function {
                name: "IsActionPressed",
                doc: "True while something bound to action `name` in the input map is held, such as `\"Jump\"`. Input the UI took doesn't count.",
                params: &[("name", "string")],
                returns: "boolean",
            },
            Function {
                name: "GetActionAxis",
                doc: "Where axis action `name` points, such as `\"Move\"`: right and up positive, at most 1 long for keys and sticks (the mouse adds its pixels on top).",
                params: &[("name", "string")],
                returns: "Vector2",
            },
            Function {
                name: "GetActionInputs",
                doc: "The names of what action `name` is bound to now, such as `{\"Space\", \"ButtonA\"}`, for a controls menu.",
                params: &[("name", "string")],
                returns: "{string}",
            },
            Function {
                name: "RebindAction",
                doc: "Binds action `name` to `inputs` instead: `Enum.KeyCode` items, mouse buttons (`Enum.UserInputType.MouseButton1`) or an axis's name such as `\"W,A,S,D\"`. The built-in controllers use it at once. It's kept as the SaveService setting `Input/<name>`, so it's saved in slots and comes back when one loads.",
                params: &[("name", "string"), ("inputs", "{any}")],
                returns: "()",
            },
        ],
    },
    ValueType {
        class: "ContextActionService",
        doc: "Functions bound to inputs, from `game:GetService(\"ContextActionService\")`. When a bound input goes down, changes or comes up, only the highest-priority action bound to it is called (the latest bound, of equal ones), so a menu can take a key from the game until it's unbound. Input the UI took doesn't reach actions. LocalScripts only.",
        fields: &[],
        operators: &[],
        methods: &[
            Function {
                name: "BindAction",
                doc: "Calls `handler(actionName, inputState, inputObject)` for `inputs` (`Enum.KeyCode` or `Enum.UserInputType` items), at the default priority (2000). Binding a name again replaces it. `createTouchButton` is kept, but no touch button is drawn yet.",
                params: &[
                    ("name", "string"),
                    ("handler", "(string, EnumUserInputState, InputObject) -> ()"),
                    ("createTouchButton", "boolean"),
                    ("...", "EnumItem"),
                ],
                returns: "()",
            },
            Function {
                name: "BindActionAtPriority",
                doc: "BindAction at `priority`: a higher number hides the same inputs from lower ones.",
                params: &[
                    ("name", "string"),
                    ("handler", "(string, EnumUserInputState, InputObject) -> ()"),
                    ("createTouchButton", "boolean"),
                    ("priority", "number"),
                    ("...", "EnumItem"),
                ],
                returns: "()",
            },
            Function {
                name: "UnbindAction",
                doc: "Unbinds action `name`, so the inputs go to the next action down.",
                params: &[("name", "string")],
                returns: "()",
            },
            Function {
                name: "UnbindAllActions",
                doc: "Unbinds every action.",
                params: &[],
                returns: "()",
            },
            Function {
                name: "GetBoundActionInfo",
                doc: "Action `name`'s `priorityLevel`, `inputTypes`, `createTouchButton` and `stackOrder`, or an empty table when it isn't bound.",
                params: &[("name", "string")],
                returns: "{[string]: any}",
            },
        ],
    },
    ValueType {
        class: "RunService",
        doc: "The game's frame loop, from `game:GetService(\"RunService\")`.",
        fields: &[
            Field {
                name: "Heartbeat",
                ty: "Signal",
                doc: "Fires once every frame, after physics. The handler gets the frame's length in seconds: `function(deltaTime) ... end`.",
            },
            Field {
                name: "Stepped",
                ty: "Signal",
                doc: "Fires once every frame, before physics. The handler gets the game time and the frame's length, in seconds: `function(time, deltaTime) ... end`.",
            },
            Field {
                name: "RenderStepped",
                ty: "Signal",
                doc: "Fires once every frame just before it's drawn, after the functions bound with `BindToRenderStep`. The handler gets the frame's length in seconds. LocalScripts only.",
            },
        ],
        operators: &[],
        methods: &[
            Function {
                name: "BindToRenderStep",
                doc: "Runs `callback` with the frame's length every frame just before it's drawn, in `priority` order with the others bound (lowest first; `Enum.RenderPriority.Camera.Value` is 200). Binding a name again replaces it. LocalScripts only.",
                params: &[
                    ("name", "string"),
                    ("priority", "number"),
                    ("callback", "(number) -> ()"),
                ],
                returns: "()",
            },
            Function {
                name: "UnbindFromRenderStep",
                doc: "Stops what was bound as `name`. LocalScripts only.",
                params: &[("name", "string")],
                returns: "()",
            },
            Function {
                name: "IsServer",
                doc: "Whether this is a Script, on the server.",
                params: &[],
                returns: "boolean",
            },
            Function {
                name: "IsClient",
                doc: "Whether this is a LocalScript, on the player's side.",
                params: &[],
                returns: "boolean",
            },
            Function {
                name: "IsStudio",
                doc: "Whether the game runs in the editor (Play, or the Console), not the player.",
                params: &[],
                returns: "boolean",
            },
            Function {
                name: "IsRunning",
                doc: "Whether the game runs: false only in the editor's Console outside Play.",
                params: &[],
                returns: "boolean",
            },
        ],
    },
    ValueType {
        class: "Debris",
        doc: "Destroys Instances later without waiting, from `game:GetService(\"Debris\")`.",
        fields: &[],
        operators: &[],
        methods: &[Function {
            name: "AddItem",
            doc: "Destroys `instance` once `seconds` (left out: 10) have passed. Nothing happens if it's destroyed before then. The script carries on at once.",
            params: &[("instance", "Instance"), ("seconds?", "number")],
            returns: "()",
        }],
    },
    ValueType {
        class: "HttpService",
        doc: "JSON and GUIDs, from `game:GetService(\"HttpService\")`. It doesn't reach the network.",
        fields: &[],
        operators: &[],
        methods: &[
            Function {
                name: "JSONEncode",
                doc: "`value` as JSON text. A table with keys 1, 2, 3 (or none) becomes an array; one with text keys an object. Numbers, strings, booleans and nil can go in; anything else, or a table inside itself, is an error.",
                params: &[("value", "any")],
                returns: "string",
            },
            Function {
                name: "JSONDecode",
                doc: "The value JSON text holds: arrays and objects become tables, `null` nil. Text that isn't JSON is an error saying the line and column of the mistake.",
                params: &[("json", "string")],
                returns: "any",
            },
            Function {
                name: "GenerateGUID",
                doc: "A new random ID such as `\"{0F8FAD5B-D9CB-469F-A165-70867728950E}\"`; `false` leaves the braces off.",
                params: &[("wrapInCurlyBraces?", "boolean")],
                returns: "string",
            },
        ],
    },
    ValueType {
        class: "PhysicsService",
        doc: "Collision groups, from `game:GetService(\"PhysicsService\")`. Changes last until Stop; the Collision Groups window (Model tab) sets the ones a game starts with.",
        fields: &[],
        operators: &[],
        methods: &[
            Function {
                name: "RegisterCollisionGroup",
                doc: "Adds a collision group (at most 32, Default included). It collides with every group until set not to. Registering one that's there does nothing.",
                params: &[("name", "string")],
                returns: "()",
            },
            Function {
                name: "CollisionGroupSetCollidable",
                doc: "Sets whether parts in the two groups collide (and touch). Both must be registered.",
                params: &[
                    ("group1", "string"),
                    ("group2", "string"),
                    ("collidable", "boolean"),
                ],
                returns: "()",
            },
            Function {
                name: "CollisionGroupsAreCollidable",
                doc: "True if parts in the two groups collide. Both must be registered.",
                params: &[("group1", "string"), ("group2", "string")],
                returns: "boolean",
            },
        ],
    },
    ValueType {
        class: "RaycastParams",
        doc: "Which parts `workspace:Raycast`, `Spherecast`, `Blockcast` and `Shapecast` find, from `RaycastParams.new()`.",
        fields: &[
            Field {
                name: "FilterDescendantsInstances",
                ty: "{Instance}",
                doc: "These Instances and everything inside them are passed by (Exclude) or the only parts found (Include). Empty at first.",
            },
            Field {
                name: "FilterType",
                ty: "EnumRaycastFilterType",
                doc: "`Enum.RaycastFilterType.Exclude` (the default) or `Include`.",
            },
            Field {
                name: "CollisionGroup",
                ty: "string",
                doc: "Only parts in groups that collide with this one are found. `\"Default\"` at first.",
            },
            Field {
                name: "RespectCanCollide",
                ty: "boolean",
                doc: "If true, parts with `CanCollide` off are passed by. False at first.",
            },
            Field {
                name: "IgnoreWater",
                ty: "boolean",
                doc: "Kept for familiarity; there's no water yet, so it changes nothing.",
            },
        ],
        operators: &[],
        methods: &[Function {
            name: "AddToFilter",
            doc: "Adds an Instance, or a list of them, to `FilterDescendantsInstances`.",
            params: &[("instances", "Instance | {Instance}")],
            returns: "()",
        }],
    },
    ValueType {
        class: "OverlapParams",
        doc: "Which parts `workspace:GetPartBoundsInBox`, `GetPartBoundsInRadius` and `GetPartsInPart` list, from `OverlapParams.new()`.",
        fields: &[
            Field {
                name: "FilterDescendantsInstances",
                ty: "{Instance}",
                doc: "These Instances and everything inside them are passed by (Exclude) or the only parts listed (Include). Empty at first.",
            },
            Field {
                name: "FilterType",
                ty: "EnumRaycastFilterType",
                doc: "`Enum.RaycastFilterType.Exclude` (the default) or `Include`.",
            },
            Field {
                name: "CollisionGroup",
                ty: "string",
                doc: "Only parts in groups that collide with this one are listed. `\"Default\"` at first.",
            },
            Field {
                name: "RespectCanCollide",
                ty: "boolean",
                doc: "If true, parts with `CanCollide` off are passed by. False at first.",
            },
            Field {
                name: "MaxParts",
                ty: "number",
                doc: "The most parts listed; 0 (the default) lists them all.",
            },
        ],
        operators: &[],
        methods: &[Function {
            name: "AddToFilter",
            doc: "Adds an Instance, or a list of them, to `FilterDescendantsInstances`.",
            params: &[("instances", "Instance | {Instance}")],
            returns: "()",
        }],
    },
    ValueType {
        class: "DataService",
        doc: "The project's data tables (`assets/data/*.data.json`, made in the Data Table tab), from `game:GetService(\"DataService\")`. Scripts read them; they can't change them.",
        fields: &[],
        operators: &[],
        methods: &[Function {
            name: "GetTable",
            doc: "The table named `name`: `\"weapons\"` is `assets/data/weapons.data.json`. A table that isn't there, or can't be read, is an error saying why.",
            params: &[("name", "string")],
            returns: "DataTable",
        }],
    },
    ValueType {
        class: "DataTable",
        doc: "A data table, read-only. Each call reads the version in use now, so an edit made during Play is what the next call sees.",
        fields: &[],
        operators: &[],
        methods: &[
            Function {
                name: "Get",
                doc: "The row with this key, or nil if there's none.",
                params: &[("key", "string")],
                returns: "DataRow?",
            },
            Function {
                name: "GetAll",
                doc: "Every row, by its key: `for key, row in weapons:GetAll() do`. A new table each call.",
                params: &[],
                returns: "{ [string]: DataRow }",
            },
            Function {
                name: "Keys",
                doc: "Every row's key, in the table's order. A new list each call.",
                params: &[],
                returns: "{ string }",
            },
        ],
    },
    ValueType {
        class: "DataRow",
        doc: "One row of a data table, read by column name: `row.Damage`. Numbers, strings, booleans, Vector3s and Color3s are what they say; an enum is its option's name, an asset its path, and a reference the row it names (nil for none). `for column, value in row do` goes through its columns. Changing it is an error: tables are read-only in scripts.",
        fields: &[],
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
        class: "CollectionService",
        doc: "Tags on Instances, from `game:GetService(\"CollectionService\")`. An Instance counts as tagged while it is in the game. Tags are saved with the world, in prefabs and in save slots, and copied by `Clone`; the editor's Tags panel lists them.",
        fields: &[],
        operators: &[],
        methods: &[
            Function {
                name: "AddTag",
                doc: "Gives `instance` the tag `tag`. Adding a tag it has does nothing.",
                params: &[("instance", "Instance"), ("tag", "string")],
                returns: "()",
            },
            Function {
                name: "RemoveTag",
                doc: "Takes the tag `tag` off `instance`.",
                params: &[("instance", "Instance"), ("tag", "string")],
                returns: "()",
            },
            Function {
                name: "HasTag",
                doc: "Whether `instance` has the tag `tag`.",
                params: &[("instance", "Instance"), ("tag", "string")],
                returns: "boolean",
            },
            Function {
                name: "GetTags",
                doc: "The tags of `instance`, in the order they were added.",
                params: &[("instance", "Instance")],
                returns: "{string}",
            },
            Function {
                name: "GetTagged",
                doc: "Every Instance in the game with the tag `tag`, parents before their children.",
                params: &[("tag", "string")],
                returns: "{Instance}",
            },
            Function {
                name: "GetAllTags",
                doc: "Every tag an Instance in the game has, sorted.",
                params: &[],
                returns: "{string}",
            },
            Function {
                name: "GetInstanceAddedSignal",
                doc: "An event that fires with an Instance when it gets the tag `tag` while in the game, or joins the game with it (such as a tagged clone put in the Workspace).",
                params: &[("tag", "string")],
                returns: "Signal",
            },
            Function {
                name: "GetInstanceRemovedSignal",
                doc: "An event that fires with an Instance when it loses the tag `tag`, or leaves the game with it (taken out, or destroyed).",
                params: &[("tag", "string")],
                returns: "Signal",
            },
        ],
    },
    ValueType {
        class: "SaveService",
        doc: "Save slots, player data, settings and checkpoints, from `game:GetService(\"SaveService\")`. Slots are files in the player's data for this game, never in the project, so saving during Play doesn't change the project. Both sides share the same data.",
        fields: &[],
        operators: &[],
        methods: &[
            Function {
                name: "SaveSlot",
                doc: "Writes save slot `slot` (1 to 999): the player data, the world state (every part and model in the Workspace, and Lighting; not scripts or ScreenGuis), the active checkpoint and the settings. Replaces what the slot held.",
                params: &[("slot", "number")],
                returns: "()",
            },
            Function {
                name: "LoadSlot",
                doc: "Puts the game back as slot `slot` was saved: player data, world state, the active checkpoint and settings. Parts and models still in the world keep being the same Instances. A slot that doesn't exist, or can't be read, is an error and changes nothing.",
                params: &[("slot", "number")],
                returns: "()",
            },
            Function {
                name: "HasSlot",
                doc: "True if slot `slot` has been saved.",
                params: &[("slot", "number")],
                returns: "boolean",
            },
            Function {
                name: "SetPlayerData",
                doc: "Keeps `value` for the player under `name`, such as `SaveService:SetPlayerData(\"Coins\", 10)`: a boolean, number, string, Vector3, Vector2, Color3, UDim2 or EnumItem. nil removes it. Saved in slots and captured at checkpoints.",
                params: &[("name", "string"), ("value", "any")],
                returns: "()",
            },
            Function {
                name: "GetPlayerData",
                doc: "The player data kept under `name`, or nil.",
                params: &[("name", "string")],
                returns: "any",
            },
            Function {
                name: "SetSetting",
                doc: "Keeps a game setting under `name`, such as `SaveService:SetSetting(\"Volume\", 0.5)`, with the same values as SetPlayerData. Saved in slots, but not captured at checkpoints. The `UI/` names are the game UI settings (see GuiService), and `Audio/MasterVolume` is `UserGameSettings.MasterVolume`.",
                params: &[("name", "string"), ("value", "any")],
                returns: "()",
            },
            Function {
                name: "GetSetting",
                doc: "The setting kept under `name`, or nil.",
                params: &[("name", "string")],
                returns: "any",
            },
            Function {
                name: "SetCheckpoint",
                doc: "Makes `checkpoint` the active Checkpoint, and captures the player data and world state now, for RestoreCheckpoint.",
                params: &[("checkpoint", "Checkpoint")],
                returns: "()",
            },
            Function {
                name: "GetCheckpoint",
                doc: "The active Checkpoint, or nil.",
                params: &[],
                returns: "Checkpoint?",
            },
            Function {
                name: "RestoreCheckpoint",
                doc: "Puts back the player data and world state captured at the active checkpoint, and stands the player character (the one whose controller owns the camera) at its Position. An error if no checkpoint is active.",
                params: &[],
                returns: "()",
            },
        ],
    },
    ValueType {
        class: "LevelService",
        doc: "The game's levels, from `game:GetService(\"LevelService\")`: `world.json` is the start level, and each other level is `levels/<name>.world.json`. A level is its Workspace; ReplicatedStorage, Players, Lighting, StarterPlayer and Teams stay the same from level to level. Both sides share it.",
        fields: &[
            Field {
                name: "CurrentLevel",
                ty: "string",
                doc: "The name of the level in the Workspace now: the start level until LoadLevel loads another (in the editor's Play, the open level).",
            },
            Field {
                name: "LevelLoaded",
                ty: "Signal",
                doc: "Fires with the level's name once LoadLevel (or LoadSlot, for a slot saved in another level) has put it in the Workspace: `function(name) ... end`. Connect again to things in the new level here.",
            },
        ],
        operators: &[],
        methods: &[
            Function {
                name: "LoadLevel",
                doc: "Puts the level `name` in place of the Workspace, such as `LevelService:LoadLevel(\"Castle\", \"Gate\")`. The Workspace's ScreenGuis and the players' characters stay; everything else in it is replaced, and the levels added with AddLevel go. Characters stand on the Instance called `spawnName` in the new level, or on one of their SpawnLocations. The loading screen shows, and LevelLoaded fires once it's in.",
                params: &[("name", "string"), ("spawnName", "string?")],
                returns: "()",
            },
            Function {
                name: "AddLevel",
                doc: "Copies the level `name` into a new Model of that name in the Workspace, beside the current level, and returns the Model. An error if it's added already.",
                params: &[("name", "string")],
                returns: "Model",
            },
            Function {
                name: "RemoveLevel",
                doc: "Takes the level `name`, added with AddLevel, out of the Workspace. An error if it isn't added.",
                params: &[("name", "string")],
                returns: "()",
            },
        ],
    },
    ValueType {
        class: "NetworkService",
        doc: "Hosting and joining games, from `game:GetService(\"NetworkService\")`, so a menu can do them. Players who join appear in `Players` on every device (the worlds aren't shared yet). Both sides have it; in the editor's Play, Host and Join fail and FindLanGames finds nothing.",
        fields: &[Field {
            name: "ConnectionFailed",
            ty: "Signal",
            doc: "Fires with why when hosting or joining fails, a join is refused (such as a version mismatch, a wrong password or a full game) or the connection is lost: `function(reason) ... end`.",
        }],
        operators: &[],
        methods: &[
            Function {
                name: "Host",
                doc: "Hosts the game on this device on UDP `port` (47800 if left out), for at most `Players.MaxPlayers` players; with a `password`, only players who give it can join. ConnectionFailed fires if the port is taken.",
                params: &[("port", "number?"), ("password", "string?")],
                returns: "()",
            },
            Function {
                name: "Join",
                doc: "Joins the game hosted at `address`, such as `NetworkService:Join(\"192.168.1.20:47800\")`, with its `password` if it has one. The player on this device may be renamed (a second `Player` is `Player2`). ConnectionFailed fires with why if it fails or is refused.",
                params: &[("address", "string"), ("password", "string?")],
                returns: "()",
            },
            Function {
                name: "FindLanGames",
                doc: "Asks the local network for hosts of this game, waits about a second (it pauses the script), and returns what answered: `{ Name, Version, Address, Players, MaxPlayers, HasPassword }` each, `Address` ready for Join.",
                params: &[],
                returns: "{ { Name: string, Version: string, Address: string, Players: number, MaxPlayers: number, HasPassword: boolean } }",
            },
            Function {
                name: "Disconnect",
                doc: "Leaves the game joined, or stops hosting; the other players leave `Players`.",
                params: &[],
                returns: "()",
            },
        ],
    },
    ValueType {
        class: "LocalizationService",
        doc: "The language UI text shows, from `game:GetService(\"LocalizationService\")`. A TextLabel or TextButton with a `TextKey` shows that key's value from the project's string table (`strings.json`, edited in the UI Designer's Strings tab) in `Language`, or its own Text when there's none. Both sides share it.",
        fields: &[
            Field {
                name: "Language",
                ty: "string",
                doc: "The active language, such as `\"English\"`: one of `Languages`, the first when the game starts. Setting it changes every bound label and button; a language the table doesn't have is an error and changes nothing.",
            },
            Field {
                name: "Languages",
                ty: "{ string }",
                doc: "The string table's languages, in order. Read only.",
            },
        ],
        operators: &[],
        methods: &[Function {
            name: "GetString",
            doc: "`key`'s value in the active language, or nil when the string table has none.",
            params: &[("key", "string")],
            returns: "string?",
        }],
    },
    ValueType {
        class: "GuiService",
        doc: "The player's game UI settings, from `game:GetService(\"GuiService\")`, for a settings menu. They change every HUD and menu (and the UI Designer's artboard) and never the 3D world, and they're separate from the editor's own appearance. Both sides share them, and save slots keep them (as the settings `UI/TextScale`, `UI/HighContrast` and `UI/ReducedMotion`).",
        fields: &[
            Field {
                name: "TextScale",
                ty: "number",
                doc: "How much bigger UI text is drawn than it's authored, from 0.5 to 3; 1 is as authored. Text with TextScaled on still fits its box. Anything else is an error and changes nothing.",
            },
            Field {
                name: "HighContrast",
                ty: "boolean",
                doc: "If true, UI text is drawn black or white (whichever is nearer its colour) and see-through backgrounds are drawn solid.",
            },
            Field {
                name: "ReducedMotion",
                ty: "boolean",
                doc: "If true, UI motion that's only for looks happens at once: CSS transitions, tweens of UI elements, and the Animations and AnimationTracks playing on them jump to where they end (a looping one holds its first frame). Events such as Completed and Stopped still fire.",
            },
        ],
        operators: &[],
        methods: &[],
    },
    ValueType {
        class: "UserGameSettings",
        doc: "This computer's graphics settings, from `game:GetService(\"UserGameSettings\")`, for a settings menu. `GraphicsQuality` picks a preset, and each other field overrides one of its values; reading a field gives the value chosen (its override, or the preset's), and setting it to nil goes back to the preset's. Changing the preset keeps the overrides. Saved on this computer by the player (never in the game); in the editor's Play they start from the editor's Graphics settings and are put back at Stop. The GPU may draw less than was chosen: the player's log and the editor's Settings say why. LocalScripts only.",
        fields: &[
            Field {
                name: "GraphicsQuality",
                ty: "EnumGraphicsQuality",
                doc: "The preset: Low, Medium, High or Ultra, or Automatic (High, or Low on a computer without a graphics card).",
            },
            Field {
                name: "ShadowQuality",
                ty: "EnumShadowQuality",
                doc: "The sun's shadow map: Off, Low (1024² × 2 cascades), Medium (2048² × 2), High (2048² × 3) or Ultra (4096² × 4), and how many spot lights cast shadows (none up to Low, then 2, 4, 8) and point lights (none up to Low, then 1, 2, 4).",
            },
            Field {
                name: "ShadowDistance",
                ty: "number",
                doc: "The farthest shadows reach, in studs: a ceiling on `Lighting.ShadowDistance`.",
            },
            Field {
                name: "PostProcessing",
                ty: "boolean",
                doc: "False turns bloom and tone mapping off, whatever Lighting asks for.",
            },
            Field {
                name: "RenderScale",
                ty: "number",
                doc: "The scene's size as a fraction of the window's, from 0.5 to 2 (kept within that). The UI is always drawn at the window's size.",
            },
            Field {
                name: "TextureQuality",
                ty: "EnumTextureQuality",
                doc: "How sharp surface textures are: Full, Half or Quarter of their width and height.",
            },
            Field {
                name: "AntiAliasing",
                ty: "EnumAntiAliasing",
                doc: "How edges are smoothed: Off, FXAA, MSAA2 or MSAA4.",
            },
            Field {
                name: "AmbientOcclusion",
                ty: "EnumAmbientOcclusion",
                doc: "How corners and creases darken their ambient light: Off, Low or High.",
            },
            Field {
                name: "ReflectionQuality",
                ty: "EnumReflectionQuality",
                doc: "How the sky is reflected and lights surfaces: Low works its gradient out per pixel (no sky image), Medium, High and Ultra bake it into a 64², 128² or 256² cubemap.",
            },
            Field {
                name: "ContactShadows",
                ty: "boolean",
                doc: "Short shadows found on screen for the small gaps the shadow maps miss, such as under a part resting on the floor. On at High and Ultra.",
            },
            Field {
                name: "RealtimeGI",
                ty: "boolean",
                doc: "Light probe volumes traced again a few probes each frame, so their bounced light follows a moving sun or changing lamps. Off at every preset, and off on a GPU without compute shaders.",
            },
            Field {
                name: "VolumetricFog",
                ty: "boolean",
                doc: "An Atmosphere's fog lit by the sun with its shadows (sun shafts) and by lamps. On at Ultra, and off on a GPU without compute shaders.",
            },
            Field {
                name: "MasterVolume",
                ty: "number",
                doc: "A multiplier on everything the game plays, from 0 (silent) to 1 (as mixed, the default). Unlike the graphics settings it's the game's: save slots keep it (as the SaveService setting `Audio/MasterVolume`), and the default audio settings UI has a slider for it.",
            },
            Field {
                name: "CloudQuality",
                ty: "EnumCloudQuality",
                doc: "How Lighting's Clouds are drawn: Low as a flat sheet, Medium, High and Ultra looked through in 16, 32 or 64 steps (High and Ultra with fine detail and light traced toward the sun). Every quality casts the same shadows.",
            },
        ],
        operators: &[],
        methods: &[],
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
        doc: "One input, as `UserInputService`'s events and `ContextActionService`'s actions get it.",
        fields: &[
            Field {
                name: "KeyCode",
                ty: "EnumKeyCode",
                doc: "Which key or controller button, such as `Enum.KeyCode.W` or `Enum.KeyCode.ButtonA` (`Thumbstick1` for the left stick), or `Unknown` for the mouse and touches.",
            },
            Field {
                name: "UserInputType",
                ty: "EnumUserInputType",
                doc: "What it came from: `Keyboard`, `MouseButton1` to `MouseButton3`, `MouseMovement`, `MouseWheel`, `Touch`, or a controller (`Gamepad1` to `Gamepad4`).",
            },
            Field {
                name: "UserInputState",
                ty: "EnumUserInputState",
                doc: "`Begin` when it went down, `Change` when it moved, `End` when it came up.",
            },
            Field {
                name: "Position",
                ty: "Vector3",
                doc: "Where the mouse or finger is, in pixels from the window's top left (Z is the wheel's notches), or a stick's position (-1 to 1, up positive).",
            },
            Field {
                name: "Delta",
                ty: "Vector3",
                doc: "How far it moved: the mouse in pixels, right and down positive, or a stick.",
            },
        ],
        operators: &[],
        methods: &[],
    },
    ValueType {
        class: "RaycastResult",
        doc: "Where `workspace:Raycast` (or a shape cast) first met a collider, or nil when it missed.",
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
        class: "Ray",
        doc: "A half-line from `Origin` along `Direction`, from `Camera:ScreenPointToRay`.",
        fields: &[
            Field {
                name: "Origin",
                ty: "Vector3",
                doc: "Where it starts.",
            },
            Field {
                name: "Direction",
                ty: "Vector3",
                doc: "Which way it goes, one stud long. Multiply it for `workspace:Raycast`: `workspace:Raycast(ray.Origin, ray.Direction * 500)`.",
            },
            Field {
                name: "Unit",
                ty: "Ray",
                doc: "The same ray: its Direction is already one stud long.",
            },
        ],
        operators: &[],
        methods: &[],
    },
    ValueType {
        class: "Mouse",
        doc: "The mouse in the world, from `Players.LocalPlayer:GetMouse()`. LocalScripts only.",
        fields: &[
            Field {
                name: "X",
                ty: "number",
                doc: "Pixels from the view's left edge.",
            },
            Field {
                name: "Y",
                ty: "number",
                doc: "Pixels from the view's top edge.",
            },
            Field {
                name: "UnitRay",
                ty: "Ray",
                doc: "The ray from the camera through the mouse (`CurrentCamera:ScreenPointToRay`).",
            },
            Field {
                name: "Target",
                ty: "Instance?",
                doc: "The part under the mouse: the first collider along UnitRay within 1000 studs, or nil.",
            },
            Field {
                name: "Hit",
                ty: "CFrame",
                doc: "Where UnitRay first meets a collider, or the point 1000 studs along it.",
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
    Function {
        name: "FindFirstChildOfClass",
        doc: "The first child whose class is exactly `className`, or nil.",
        params: &[("className", "string")],
        returns: "Instance?",
    },
    Function {
        name: "FindFirstChildWhichIsA",
        doc: "The first child that `IsA(className)`, or nil. With `recursive`, searches every descendant.",
        params: &[("className", "string"), ("recursive?", "boolean")],
        returns: "Instance?",
    },
    Function {
        name: "FindFirstAncestor",
        doc: "The nearest Instance above this one with this name, or nil.",
        params: &[("name", "string")],
        returns: "Instance?",
    },
    Function {
        name: "FindFirstAncestorOfClass",
        doc: "The nearest Instance above this one whose class is exactly `className`, or nil.",
        params: &[("className", "string")],
        returns: "Instance?",
    },
    Function {
        name: "FindFirstAncestorWhichIsA",
        doc: "The nearest Instance above this one that `IsA(className)`, or nil.",
        params: &[("className", "string")],
        returns: "Instance?",
    },
    Function {
        name: "WaitForChild",
        doc: "The child with this name, waiting (as `task.wait` does) until there is one. With `timeout`, gives nil after that many seconds. Without one, warns once after 5 seconds and keeps waiting.",
        params: &[("name", "string"), ("timeout?", "number")],
        returns: "Instance?",
    },
    Function {
        name: "GetChildren",
        doc: "The Instances directly inside this one, in order.",
        params: &[],
        returns: "{Instance}",
    },
    Function {
        name: "GetDescendants",
        doc: "Every Instance inside this one, parents before their children.",
        params: &[],
        returns: "{Instance}",
    },
    Function {
        name: "IsA",
        doc: "Whether this Instance's class is `className` or inherits from it: a Part and a MeshPart are both a `\"BasePart\"`, and everything is an `\"Instance\"`.",
        params: &[("className", "string")],
        returns: "boolean",
    },
    Function {
        name: "IsDescendantOf",
        doc: "Whether this Instance is somewhere inside `ancestor`.",
        params: &[("ancestor", "Instance")],
        returns: "boolean",
    },
    Function {
        name: "IsAncestorOf",
        doc: "Whether `descendant` is somewhere inside this Instance.",
        params: &[("descendant", "Instance")],
        returns: "boolean",
    },
    Function {
        name: "GetFullName",
        doc: "The names from the service down to this Instance, joined by dots, such as `\"Workspace.Model.Part\"`.",
        params: &[],
        returns: "string",
    },
    Function {
        name: "Clone",
        doc: "A copy of this Instance and everything inside it, with no Parent. A copied Script starts once it's put where scripts run (the Workspace or ServerScriptService; a LocalScript also in StarterPlayerScripts or Players). Services can't be cloned.",
        params: &[],
        returns: "Instance",
    },
    Function {
        name: "Destroy",
        doc: "Removes this Instance and everything inside it for good, firing `Destroying` first. Its Parent can't be set again, and a script inside it stops. Services can't be destroyed; destroying twice is harmless.",
        params: &[],
        returns: "()",
    },
    Function {
        name: "ClearAllChildren",
        doc: "Destroys every child of this Instance (except services).",
        params: &[],
        returns: "()",
    },
    Function {
        name: "SetAttribute",
        doc: "Sets the attribute `name` to `value`: a boolean, number, string, Vector3, Vector2, Color3, UDim2, CFrame or EnumItem; nil removes it. Names are 1 to 100 letters, digits and underscores. Attributes are saved with the world, in prefabs and in save slots, and copied by `Clone`.",
        params: &[("name", "string"), ("value", "any")],
        returns: "()",
    },
    Function {
        name: "GetAttribute",
        doc: "The value of the attribute `name`, or nil if it has none.",
        params: &[("name", "string")],
        returns: "any",
    },
    Function {
        name: "GetAttributes",
        doc: "Every attribute, as a table from name to value.",
        params: &[],
        returns: "{ [string]: any }",
    },
    Function {
        name: "GetAttributeChangedSignal",
        doc: "An event that fires when the attribute `name` gets a different value or is removed. The handler gets nothing: read it with `GetAttribute`.",
        params: &[("name", "string")],
        returns: "Signal",
    },
    Function {
        name: "AddTag",
        doc: "Gives this Instance the tag `tag` (same as `CollectionService:AddTag`). Adding a tag it has does nothing.",
        params: &[("tag", "string")],
        returns: "()",
    },
    Function {
        name: "RemoveTag",
        doc: "Takes the tag `tag` off this Instance (same as `CollectionService:RemoveTag`).",
        params: &[("tag", "string")],
        returns: "()",
    },
    Function {
        name: "HasTag",
        doc: "Whether this Instance has the tag `tag`.",
        params: &[("tag", "string")],
        returns: "boolean",
    },
    Function {
        name: "GetTags",
        doc: "This Instance's tags, in the order they were added.",
        params: &[],
        returns: "{string}",
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
    (
        "DescendantAdded",
        "Fires when an Instance is put anywhere inside this one, once for it and once for each Instance inside it. The handler gets that descendant.",
    ),
    (
        "DescendantRemoving",
        "Fires when an Instance is taken out from anywhere inside this one, once for it and once for each Instance inside it. The handler gets that descendant.",
    ),
    (
        "AncestryChanged",
        "Fires when this Instance or one of its ancestors gets a new Parent. The handler gets the Instance that moved and its new Parent (nil when it was taken out or destroyed).",
    ),
    (
        "Destroying",
        "Fires once when `Destroy` is called on this Instance or one of its ancestors, while it can still be read.",
    ),
    (
        "AttributeChanged",
        "Fires when an attribute gets a different value or is removed. The handler gets the attribute's name.",
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
const PART_CFRAME_DOC: &str =
    "Its Position and Orientation as one CFrame. Setting it sets both; it isn't saved itself.";
const GET_PIVOT_DOC: &str = "Where it turns and moves from: a part's own CFrame, or for a Model the centre of the box around its parts, unturned.";
const APPLY_IMPULSE_DOC: &str = "Pushes the part through its center of mass: its velocity changes by `impulse` divided by its assembly's mass (`AssemblyMass`, which is `GetMass()` for a part alone). Pushes in one frame add up. Anchored parts don't move.";
const CONNECTED_PARTS_DOC: &str = "The part, then the parts welded to it by an enabled WeldConstraint; with `recursive` true, every part in its assembly.";
const SET_OWNER_DOC: &str = "Scripts only (Phase 66): `player`'s device simulates this part's whole assembly from now on, so it responds at once for them, and the server sees what it does; nil gives it back to the server. An anchored part has no owner (an error).";
const GET_OWNER_DOC: &str = "Scripts only: the Player whose device simulates this part's assembly now, or nil for the server. An anchored part has no owner (an error).";
const SET_AUTO_DOC: &str = "Scripts only: the engine chooses who simulates this part's assembly: the player driving a VehicleSeat in it, else the nearest player within 40 studs, else the server.";
const GET_AUTO_DOC: &str =
    "Scripts only: true after SetNetworkOwnershipAuto, until SetNetworkOwner.";
const ROOT_PART_DOC: &str = "The root of its assembly: an anchored part if there is one, then one that isn't Massless, then the largest. A part alone is its own root.";
const APPLY_IMPULSE_AT_DOC: &str = "Pushes the part at `position` in the world: it speeds up as `ApplyImpulse` would, and an impulse off its center of mass also spins it.";
const APPLY_ANGULAR_DOC: &str = "Spins the part: its `AssemblyAngularVelocity` changes by `impulse` (radians a second times inertia) through its inertia.";
const GET_MASS_DOC: &str = "Its density times its volume: a 1×1×1 Plastic block weighs 1. MeshParts weigh their collision shape.";
const ASSEMBLY_MASS_DOC: &str = "The mass of everything moving with it: the parts welded into its assembly, except Massless ones (a part alone keeps its own mass). Read-only.";
const CENTER_OF_MASS_DOC: &str =
    "Where its mass is centered, in the world: the middle of a block or ball. Read-only.";
const CURRENT_PHYSICAL_DOC: &str = "The physical properties in use: `CustomPhysicalProperties`, or its Material's when that's nil. Read-only.";
const PIVOT_TO_DOC: &str = "Moves and turns it so its pivot is at `target`. A Model's parts all move together and keep their places relative to each other.";

const UNION_ASYNC_DOC: &str = "Joins the part and `parts` into one solid and returns it as a new UnionOperation (its Parent is nil), with this part's look and physics. NegateOperations among `parts` are cut out. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid (a Capsule, or a model with holes). `options.CollisionFidelity` sets the union's.";
const SUBTRACT_ASYNC_DOC: &str = "Cuts every one of `parts` out of the part and returns what's left as a new UnionOperation (its Parent is nil), with this part's look and physics: a crater where a ball was. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid or nothing is left. `options.CollisionFidelity` sets the union's.";
const INTERSECT_ASYNC_DOC: &str = "Keeps only where the part and all of `parts` overlap, as a new UnionOperation (its Parent is nil), with this part's look and physics. The work runs in the background and the script waits for it. Errors, saying why, if a part isn't a closed solid or they don't overlap. `options.CollisionFidelity` sets the union's.";

pub(super) const CLASS_METHODS: &[(ClassName, Function)] = &[
    (
        ClassName::Part,
        Function {
            name: "UnionAsync",
            doc: UNION_ASYNC_DOC,
            params: &[("parts", "{Instance}"), ("options", "{[string]: any}?")],
            returns: "UnionOperation",
        },
    ),
    (
        ClassName::Part,
        Function {
            name: "SubtractAsync",
            doc: SUBTRACT_ASYNC_DOC,
            params: &[("parts", "{Instance}"), ("options", "{[string]: any}?")],
            returns: "UnionOperation",
        },
    ),
    (
        ClassName::Part,
        Function {
            name: "IntersectAsync",
            doc: INTERSECT_ASYNC_DOC,
            params: &[("parts", "{Instance}"), ("options", "{[string]: any}?")],
            returns: "UnionOperation",
        },
    ),
    (
        ClassName::MeshPart,
        Function {
            name: "UnionAsync",
            doc: UNION_ASYNC_DOC,
            params: &[("parts", "{Instance}"), ("options", "{[string]: any}?")],
            returns: "UnionOperation",
        },
    ),
    (
        ClassName::MeshPart,
        Function {
            name: "SubtractAsync",
            doc: SUBTRACT_ASYNC_DOC,
            params: &[("parts", "{Instance}"), ("options", "{[string]: any}?")],
            returns: "UnionOperation",
        },
    ),
    (
        ClassName::MeshPart,
        Function {
            name: "IntersectAsync",
            doc: INTERSECT_ASYNC_DOC,
            params: &[("parts", "{Instance}"), ("options", "{[string]: any}?")],
            returns: "UnionOperation",
        },
    ),
    (
        ClassName::Seat,
        Function {
            name: "Sit",
            doc: "Sits `controller`'s character on the seat at its next step, even if the seat is Disabled. Whoever sat there stands up, and the character leaves any other seat.",
            params: &[("controller", "Instance")],
            returns: "()",
        },
    ),
    (
        ClassName::Part,
        Function {
            name: "GetConnectedParts",
            doc: CONNECTED_PARTS_DOC,
            params: &[("recursive", "boolean?")],
            returns: "{Instance}",
        },
    ),
    (
        ClassName::Part,
        Function {
            name: "GetRootPart",
            doc: ROOT_PART_DOC,
            params: &[],
            returns: "Instance",
        },
    ),
    (
        ClassName::MeshPart,
        Function {
            name: "GetConnectedParts",
            doc: CONNECTED_PARTS_DOC,
            params: &[("recursive", "boolean?")],
            returns: "{Instance}",
        },
    ),
    (
        ClassName::MeshPart,
        Function {
            name: "GetRootPart",
            doc: ROOT_PART_DOC,
            params: &[],
            returns: "Instance",
        },
    ),
    (
        ClassName::Camera,
        Function {
            name: "ScreenPointToRay",
            doc: "`CurrentCamera` only: the ray from the camera through pixel `x`, `y` (from the view's top left, like `GetMouseLocation`), starting `depth` studs (0 if left out) in front.",
            params: &[("x", "number"), ("y", "number"), ("depth", "number?")],
            returns: "Ray",
        },
    ),
    (
        ClassName::Camera,
        Function {
            name: "ViewportPointToRay",
            doc: "`CurrentCamera` only: the same as ScreenPointToRay; nothing covers part of the screen.",
            params: &[("x", "number"), ("y", "number"), ("depth", "number?")],
            returns: "Ray",
        },
    ),
    (
        ClassName::Camera,
        Function {
            name: "WorldToScreenPoint",
            doc: "`CurrentCamera` only: where `point` shows, in pixels from the view's top left (Z: studs in front of the camera, negative behind it), and whether it's on screen. Put a UI element there with `UDim2.fromOffset(at.X, at.Y)`.",
            params: &[("point", "Vector3")],
            returns: "(Vector3, boolean)",
        },
    ),
    (
        ClassName::Camera,
        Function {
            name: "WorldToViewportPoint",
            doc: "`CurrentCamera` only: the same as WorldToScreenPoint.",
            params: &[("point", "Vector3")],
            returns: "(Vector3, boolean)",
        },
    ),
    (
        ClassName::Camera,
        Function {
            name: "Shake",
            doc: "`CurrentCamera` only: shakes the view for `duration` seconds, fading out, on top of any CameraType. An `intensity` of 1 moves it about a quarter of a stud and turns it a degree; shakes add up.",
            params: &[("intensity", "number"), ("duration", "number")],
            returns: "()",
        },
    ),
    (
        ClassName::Player,
        Function {
            name: "GetMouse",
            doc: "LocalScripts only: the mouse in the world, with the part under it (`Target`).",
            params: &[],
            returns: "Mouse",
        },
    ),
    (
        ClassName::Part,
        Function {
            name: "GetPivot",
            doc: GET_PIVOT_DOC,
            params: &[],
            returns: "CFrame",
        },
    ),
    (
        ClassName::Part,
        Function {
            name: "PivotTo",
            doc: PIVOT_TO_DOC,
            params: &[("target", "CFrame")],
            returns: "()",
        },
    ),
    (
        ClassName::MeshPart,
        Function {
            name: "GetPivot",
            doc: GET_PIVOT_DOC,
            params: &[],
            returns: "CFrame",
        },
    ),
    (
        ClassName::MeshPart,
        Function {
            name: "PivotTo",
            doc: PIVOT_TO_DOC,
            params: &[("target", "CFrame")],
            returns: "()",
        },
    ),
    (
        ClassName::Fracture,
        Function {
            name: "Shatter",
            doc: "Replaces the part with its pieces before the next physics step, flying away from `point` (the part's middle if nil) at up to `force` ÷ the part's mass studs a second: that much for the nearest pieces, half for the farthest. Fires Shattered. Does nothing until Generate has made the pieces.",
            params: &[("point", "Vector3?"), ("force", "number?")],
            returns: "()",
        },
    ),
    (
        ClassName::Health,
        Function {
            name: "TakeDamage",
            doc: "Takes `amount` from Health (never below 0), unless it's Invulnerable or already dead. Fires HealthChanged, then Damaged with `amount` and `source`, then Died if it reached 0. Starts the RegenDelay wait again.",
            params: &[("amount", "number"), ("source", "Instance?")],
            returns: "()",
        },
    ),
    (
        ClassName::Inventory,
        Function {
            name: "GetItems",
            doc: "The Tools in it, in hotbar order, the one in hand included.",
            params: &[],
            returns: "{Tool}",
        },
    ),
    (
        ClassName::Inventory,
        Function {
            name: "Add",
            doc: "Puts `tool` in it, taking it out of any other Inventory first. Returns false, leaving the Tool where it is, when it already holds Capacity Tools; true if it's in it.",
            params: &[("tool", "Tool")],
            returns: "boolean",
        },
    ),
    (
        ClassName::Inventory,
        Function {
            name: "Remove",
            doc: "Takes `tool` out of it (out of the hand first), leaving its Parent nil. Returns false if it wasn't in it.",
            params: &[("tool", "Tool")],
            returns: "boolean",
        },
    ),
    (
        ClassName::Inventory,
        Function {
            name: "Equip",
            doc: "Puts `tool`, which must be in it, in the character's right hand, putting away the one there. Fires Unequipped and Equipped, and sets the character's Animator's `Equipped` Bool parameter if it has one.",
            params: &[("tool", "Tool")],
            returns: "()",
        },
    ),
    (
        ClassName::Inventory,
        Function {
            name: "UnequipAll",
            doc: "Puts away the Tool in hand, if any, firing its Unequipped.",
            params: &[],
            returns: "()",
        },
    ),
    (
        ClassName::Tool,
        Function {
            name: "Drop",
            doc: "Takes it out of its Inventory (and the hand) and puts its Handle on the ground in front of the character, in the Workspace. The character picks it up again once it has stepped off it and touches it. Returns false if it's not carried or CanBeDropped is off.",
            params: &[],
            returns: "boolean",
        },
    ),
    (
        ClassName::Part,
        Function {
            name: "SetNetworkOwner",
            doc: SET_OWNER_DOC,
            params: &[("player", "Instance?")],
            returns: "()",
        },
    ),
    (
        ClassName::Part,
        Function {
            name: "GetNetworkOwner",
            doc: GET_OWNER_DOC,
            params: &[],
            returns: "Instance?",
        },
    ),
    (
        ClassName::Part,
        Function {
            name: "SetNetworkOwnershipAuto",
            doc: SET_AUTO_DOC,
            params: &[],
            returns: "()",
        },
    ),
    (
        ClassName::Part,
        Function {
            name: "GetNetworkOwnershipAuto",
            doc: GET_AUTO_DOC,
            params: &[],
            returns: "boolean",
        },
    ),
    (
        ClassName::MeshPart,
        Function {
            name: "SetNetworkOwner",
            doc: SET_OWNER_DOC,
            params: &[("player", "Instance?")],
            returns: "()",
        },
    ),
    (
        ClassName::MeshPart,
        Function {
            name: "GetNetworkOwner",
            doc: GET_OWNER_DOC,
            params: &[],
            returns: "Instance?",
        },
    ),
    (
        ClassName::MeshPart,
        Function {
            name: "SetNetworkOwnershipAuto",
            doc: SET_AUTO_DOC,
            params: &[],
            returns: "()",
        },
    ),
    (
        ClassName::MeshPart,
        Function {
            name: "GetNetworkOwnershipAuto",
            doc: GET_AUTO_DOC,
            params: &[],
            returns: "boolean",
        },
    ),
    (
        ClassName::Player,
        Function {
            name: "Kick",
            doc: "Scripts only (Phase 67): takes the player out of the game. Their device's NetworkService.ConnectionFailed fires with `reason` (\"You were kicked from the game.\" without one), and PlayerRemoving fires here. Kicking the player on this device ends the game, with the reason.",
            params: &[("reason", "string?")],
            returns: "()",
        },
    ),
    (
        ClassName::Player,
        Function {
            name: "LoadCharacter",
            doc: "Scripts only: gives the player a new character. The old one (if any) leaves the Workspace, fires CharacterRemoving and is destroyed; a copy of the StarterCharacter stands on one of the player's team's SpawnLocations (or a Neutral one), chosen at random, with a ForceField for its Duration, and CharacterAdded fires. With no SpawnLocation it stands at the origin and Output warns.",
            params: &[],
            returns: "()",
        },
    ),
    (
        ClassName::Players,
        Function {
            name: "GetPlayers",
            doc: "Every Player in the game.",
            params: &[],
            returns: "{Player}",
        },
    ),
    (
        ClassName::Team,
        Function {
            name: "GetPlayers",
            doc: "The Players on this team (their Team is it, and Neutral is off).",
            params: &[],
            returns: "{Player}",
        },
    ),
    (
        ClassName::Humanoid,
        Function {
            name: "MoveTo",
            doc: "Walks the character to `position` along a path on the navigation mesh, by its controller's MoveDirection, and fires MoveToFinished(true) when it's there (within half a stud across the ground), or MoveToFinished(false) if there's no path, 8 seconds pass or it dies. With `part`, it walks to the place `position` was relative to the part, as the part moves. A new MoveTo, or Move, replaces it.",
            params: &[("position", "Vector3"), ("part", "BasePart?")],
            returns: "()",
        },
    ),
    (
        ClassName::Humanoid,
        Function {
            name: "Move",
            doc: "Sets the controller's MoveDirection to `direction` across the ground (relative to where the camera looks if `relativeToCamera`, on the client), and stops any MoveTo. A zero vector stops.",
            params: &[("direction", "Vector3"), ("relativeToCamera", "boolean?")],
            returns: "()",
        },
    ),
    (
        ClassName::Humanoid,
        Function {
            name: "Jump",
            doc: "Jumps once from the ground (the controller's Jump).",
            params: &[],
            returns: "()",
        },
    ),
    (
        ClassName::Humanoid,
        Function {
            name: "TakeDamage",
            doc: "Its Health's TakeDamage(amount, source).",
            params: &[("amount", "number"), ("source", "Instance?")],
            returns: "()",
        },
    ),
    (
        ClassName::Humanoid,
        Function {
            name: "EquipTool",
            doc: "Puts `tool` in the character's Inventory (an error if it's full) and in its hand.",
            params: &[("tool", "Tool")],
            returns: "()",
        },
    ),
    (
        ClassName::Humanoid,
        Function {
            name: "UnequipTools",
            doc: "Puts away the Tool in hand (its Inventory's UnequipAll).",
            params: &[],
            returns: "()",
        },
    ),
    (
        ClassName::Humanoid,
        Function {
            name: "GetState",
            doc: "The character's state, as of the last frame (Running before the first).",
            params: &[],
            returns: "EnumItem",
        },
    ),
    (
        ClassName::Humanoid,
        Function {
            name: "ChangeState",
            doc: "Asks for a state: Running stands it up (off a Seat, out of PlatformStand, its Ragdolls off), Jumping jumps, Ragdoll turns its Ragdolls on, and Dead sets its Health to 0. The others follow the character and are an error.",
            params: &[("state", "EnumItem")],
            returns: "()",
        },
    ),
    (
        ClassName::Health,
        Function {
            name: "Heal",
            doc: "Adds `amount` to Health, up to MaxHealth, and fires HealthChanged. Does nothing once it's dead (Health 0): set Health to bring it back.",
            params: &[("amount", "number")],
            returns: "()",
        },
    ),
    (
        ClassName::Ragdoll,
        Function {
            name: "ApplyImpulse",
            doc: "Pushes the body of the bone named `bone` (`\"Head\"` finds `mixamorig:Head`) through its center of mass: its velocity changes by `impulse` over its mass. Turn the ragdoll on in the same frame (or before) for the push to knock it.",
            params: &[("bone", "string"), ("impulse", "Vector3")],
            returns: "()",
        },
    ),
    (
        ClassName::Part,
        Function {
            name: "ApplyImpulse",
            doc: APPLY_IMPULSE_DOC,
            params: &[("impulse", "Vector3")],
            returns: "()",
        },
    ),
    (
        ClassName::Part,
        Function {
            name: "ApplyImpulseAtPosition",
            doc: APPLY_IMPULSE_AT_DOC,
            params: &[("impulse", "Vector3"), ("position", "Vector3")],
            returns: "()",
        },
    ),
    (
        ClassName::Part,
        Function {
            name: "ApplyAngularImpulse",
            doc: APPLY_ANGULAR_DOC,
            params: &[("impulse", "Vector3")],
            returns: "()",
        },
    ),
    (
        ClassName::Part,
        Function {
            name: "GetMass",
            doc: GET_MASS_DOC,
            params: &[],
            returns: "number",
        },
    ),
    (
        ClassName::MeshPart,
        Function {
            name: "ApplyImpulse",
            doc: APPLY_IMPULSE_DOC,
            params: &[("impulse", "Vector3")],
            returns: "()",
        },
    ),
    (
        ClassName::MeshPart,
        Function {
            name: "ApplyImpulseAtPosition",
            doc: APPLY_IMPULSE_AT_DOC,
            params: &[("impulse", "Vector3"), ("position", "Vector3")],
            returns: "()",
        },
    ),
    (
        ClassName::MeshPart,
        Function {
            name: "ApplyAngularImpulse",
            doc: APPLY_ANGULAR_DOC,
            params: &[("impulse", "Vector3")],
            returns: "()",
        },
    ),
    (
        ClassName::MeshPart,
        Function {
            name: "GetMass",
            doc: GET_MASS_DOC,
            params: &[],
            returns: "number",
        },
    ),
    (
        ClassName::Model,
        Function {
            name: "GetPivot",
            doc: GET_PIVOT_DOC,
            params: &[],
            returns: "CFrame",
        },
    ),
    (
        ClassName::Model,
        Function {
            name: "PivotTo",
            doc: PIVOT_TO_DOC,
            params: &[("target", "CFrame")],
            returns: "()",
        },
    ),
    (
        ClassName::DataModel,
        Function {
            name: "GetService",
            doc: "The service with this class name, such as `\"Players\"`, `\"ReplicatedFirst\"`, `\"RunService\"`, `\"SaveService\"`, `\"LevelService\"`, `\"NetworkService\"`, `\"LocalizationService\"`, `\"GuiService\"`, `\"Debris\"`, `\"HttpService\"`, `\"DataService\"`, `\"PhysicsService\"`, `\"TweenService\"`, `\"CollectionService\"`, `\"UserGameSettings\"`, `\"UserInputService\"` or `\"ContextActionService\"`.",
            params: &[("className", "string")],
            returns: "Instance",
        },
    ),
    (
        ClassName::RemoteFunction,
        Function {
            name: "InvokeServer",
            doc: "Asks the server: pauses until its `OnServerInvoke` returns (it gets this player first, then the arguments), and returns what it returned. An error there is raised here, with the server script's line. LocalScripts only.",
            params: &[("...", "any")],
            returns: "...any",
        },
    ),
    (
        ClassName::RemoteFunction,
        Function {
            name: "InvokeClient",
            doc: "Asks one player's device: pauses until its `OnClientInvoke` returns (it gets the arguments), and returns what it returned. An error there is raised here, with the LocalScript's line. A player who leaves first never answers, so the Script waits.",
            params: &[("player", "Player"), ("...", "any")],
            returns: "...any",
        },
    ),
    (
        ClassName::BindableEvent,
        Function {
            name: "Fire",
            doc: "Fires `Event` with the arguments, for scripts on this side.",
            params: &[("...", "any")],
            returns: "()",
        },
    ),
    (
        ClassName::BindableFunction,
        Function {
            name: "Invoke",
            doc: "Calls `OnInvoke` with the arguments and returns what it returns, waiting while it waits. Its errors are raised here.",
            params: &[("...", "any")],
            returns: "...any",
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
            doc: "The first collider along `direction` from `origin`, or nil. The direction's length is how far the ray reaches. Hits sensors (`CanCollide` off) as well as solid parts and terrain, but not parts with `CanQuery` off. `params` (a `RaycastParams`) can pass by or keep to some parts, a collision group, or sensors. The result has Instance, Position, Normal and Distance.",
            params: &[
                ("origin", "Vector3"),
                ("direction", "Vector3"),
                ("params?", "RaycastParams"),
            ],
            returns: "RaycastResult?",
        },
    ),
    (
        ClassName::Workspace,
        Function {
            name: "Spherecast",
            doc: "Sweeps a ball of `radius` from `position` along `direction` (its length is how far) and returns the first part it meets, as `Raycast` does, or nil. Parts it starts inside aren't hit.",
            params: &[
                ("position", "Vector3"),
                ("radius", "number"),
                ("direction", "Vector3"),
                ("params?", "RaycastParams"),
            ],
            returns: "RaycastResult?",
        },
    ),
    (
        ClassName::Workspace,
        Function {
            name: "Blockcast",
            doc: "Sweeps a box of `size` placed at `cframe` along `direction` and returns the first part it meets, as `Raycast` does, or nil. Parts it starts inside aren't hit.",
            params: &[
                ("cframe", "CFrame"),
                ("size", "Vector3"),
                ("direction", "Vector3"),
                ("params?", "RaycastParams"),
            ],
            returns: "RaycastResult?",
        },
    ),
    (
        ClassName::Workspace,
        Function {
            name: "Shapecast",
            doc: "Sweeps `part`'s own shape from where it is along `direction` and returns the first other part it meets, as `Raycast` does, or nil. The part doesn't move.",
            params: &[
                ("part", "Instance"),
                ("direction", "Vector3"),
                ("params?", "RaycastParams"),
            ],
            returns: "RaycastResult?",
        },
    ),
    (
        ClassName::Workspace,
        Function {
            name: "GetPartBoundsInBox",
            doc: "The parts whose bounding boxes overlap the box of `size` at `cframe`, in Explorer order. Terrain isn't listed.",
            params: &[
                ("cframe", "CFrame"),
                ("size", "Vector3"),
                ("params?", "OverlapParams"),
            ],
            returns: "{Instance}",
        },
    ),
    (
        ClassName::Workspace,
        Function {
            name: "GetPartBoundsInRadius",
            doc: "The parts whose bounding boxes are within `radius` studs of `position`, in Explorer order. Terrain isn't listed.",
            params: &[
                ("position", "Vector3"),
                ("radius", "number"),
                ("params?", "OverlapParams"),
            ],
            returns: "{Instance}",
        },
    ),
    (
        ClassName::Workspace,
        Function {
            name: "GetPartsInPart",
            doc: "The parts whose shapes overlap `part`'s, not counting it, in Explorer order: what's inside a trigger zone. Terrain isn't listed.",
            params: &[("part", "Instance"), ("params?", "OverlapParams")],
            returns: "{Instance}",
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
        ClassName::Sequence,
        Function {
            name: "Play",
            doc: "Plays it: from its start if it hasn't played, has ended or was stopped; from where it is if it's paused.",
            params: &[],
            returns: "()",
        },
    ),
    (
        ClassName::Sequence,
        Function {
            name: "Pause",
            doc: "Holds it where it is, with the camera and everything it keys still there. Play carries on.",
            params: &[],
            returns: "()",
        },
    ),
    (
        ClassName::Sequence,
        Function {
            name: "Stop",
            doc: "Stops it and goes back to its start: its clips, Sounds, fades, subtitles and levels go, and the camera comes back. Keyed properties stay where they are. Completed doesn't fire.",
            params: &[],
            returns: "()",
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
        ClassName::RemoteFunction,
        Field {
            name: "OnServerInvoke",
            ty: "(Player, ...any) -> ...any",
            doc: "The function that answers `InvokeServer`: `remote.OnServerInvoke = function(player, ...) return ... end`. Setting it replaces the one before; it can be set, not read. Questions asked before it's set wait for it.",
        },
    ),
    (
        ClassName::RemoteFunction,
        Field {
            name: "OnClientInvoke",
            ty: "(...any) -> ...any",
            doc: "The function that answers a Script's `InvokeClient`: `remote.OnClientInvoke = function(...) return ... end`. Setting it replaces the one before; it can be set, not read. Questions asked before it's set wait for it.",
        },
    ),
    (
        ClassName::BindableFunction,
        Field {
            name: "OnInvoke",
            ty: "(...any) -> ...any",
            doc: "The function `Invoke` calls: `func.OnInvoke = function(...) return ... end`. Setting it replaces the one before; it can be set, not read. Each side has its own.",
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "Debris",
            ty: "Debris",
            doc: "Destroys Instances later (same as `game:GetService(\"Debris\")`).",
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "HttpService",
            ty: "HttpService",
            doc: "JSON and GUIDs (same as `game:GetService(\"HttpService\")`).",
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "DataService",
            ty: "DataService",
            doc: "The project's data tables (same as `game:GetService(\"DataService\")`).",
        },
    ),
    (
        ClassName::Camera,
        Field {
            name: "CFrame",
            ty: "CFrame",
            doc: "Its Position and Orientation as one CFrame. `workspace.CurrentCamera.CFrame = viewpoint.CFrame` cuts to a viewpoint; TweenService moves it smoothly.",
        },
    ),
    (
        ClassName::Camera,
        Field {
            name: "LookVector",
            ty: "Vector3",
            doc: "`CurrentCamera` only: the direction it looks in, one stud long. Read-only.",
        },
    ),
    (
        ClassName::Camera,
        Field {
            name: "CameraType",
            ty: "EnumCameraType",
            doc: "`CurrentCamera` only: who moves it. `Custom` (the default) is the character controller's camera, or the fly camera; `Scriptable` leaves CFrame entirely to scripts; `Follow` and `Track` follow CameraSubject from where the camera was (Follow turning as the subject turns, Track not); `Fixed` stays put. Back to Custom, the controller has it again.",
        },
    ),
    (
        ClassName::Camera,
        Field {
            name: "CameraSubject",
            ty: "Instance?",
            doc: "`CurrentCamera` only: what Follow and Track follow: a Model, a part, or a character controller or agent (its body). Nil follows nothing.",
        },
    ),
    (
        ClassName::Camera,
        Field {
            name: "Focus",
            ty: "CFrame",
            doc: "`CurrentCamera` only: what it's looking at: what a script set, otherwise CameraSubject, otherwise 10 studs in front of it.",
        },
    ),
    (
        ClassName::Camera,
        Field {
            name: "ViewportSize",
            ty: "Vector2",
            doc: "`CurrentCamera` only: the view's size in pixels (the window, or the editor's viewport). Read-only.",
        },
    ),
    (
        ClassName::Attachment,
        Field {
            name: "CFrame",
            ty: "CFrame",
            doc: "Its Position and Orientation on its part as one CFrame. Setting it sets both.",
        },
    ),
    (
        ClassName::Attachment,
        Field {
            name: "WorldCFrame",
            ty: "CFrame",
            doc: "Where it is in the world: its part's CFrame times its own. Setting it moves it on its part.",
        },
    ),
    (
        ClassName::Attachment,
        Field {
            name: "WorldPosition",
            ty: "Vector3",
            doc: "Where it is in the world. Setting it moves it on its part.",
        },
    ),
    (
        ClassName::Attachment,
        Field {
            name: "WorldOrientation",
            ty: "Vector3",
            doc: "How it's turned in the world, in degrees. Setting it turns it on its part.",
        },
    ),
    (
        ClassName::Attachment,
        Field {
            name: "Axis",
            ty: "Vector3",
            doc: "The way its X points, relative to its part. Setting it turns the attachment, keeping SecondaryAxis as close as it can.",
        },
    ),
    (
        ClassName::Attachment,
        Field {
            name: "SecondaryAxis",
            ty: "Vector3",
            doc: "The way its Y points, relative to its part. Setting it turns the attachment around its Axis.",
        },
    ),
    (
        ClassName::Attachment,
        Field {
            name: "WorldAxis",
            ty: "Vector3",
            doc: "The way its X points in the world. Read-only.",
        },
    ),
    (
        ClassName::Attachment,
        Field {
            name: "WorldSecondaryAxis",
            ty: "Vector3",
            doc: "The way its Y points in the world. Read-only.",
        },
    ),
    (
        ClassName::Part,
        Field {
            name: "CFrame",
            ty: "CFrame",
            doc: PART_CFRAME_DOC,
        },
    ),
    (
        ClassName::MeshPart,
        Field {
            name: "CFrame",
            ty: "CFrame",
            doc: PART_CFRAME_DOC,
        },
    ),
    (
        ClassName::Part,
        Field {
            name: "AssemblyMass",
            ty: "number",
            doc: ASSEMBLY_MASS_DOC,
        },
    ),
    (
        ClassName::Part,
        Field {
            name: "AssemblyCenterOfMass",
            ty: "Vector3",
            doc: CENTER_OF_MASS_DOC,
        },
    ),
    (
        ClassName::Part,
        Field {
            name: "CurrentPhysicalProperties",
            ty: "PhysicalProperties",
            doc: CURRENT_PHYSICAL_DOC,
        },
    ),
    (
        ClassName::MeshPart,
        Field {
            name: "AssemblyMass",
            ty: "number",
            doc: ASSEMBLY_MASS_DOC,
        },
    ),
    (
        ClassName::MeshPart,
        Field {
            name: "AssemblyCenterOfMass",
            ty: "Vector3",
            doc: CENTER_OF_MASS_DOC,
        },
    ),
    (
        ClassName::MeshPart,
        Field {
            name: "CurrentPhysicalProperties",
            ty: "PhysicalProperties",
            doc: CURRENT_PHYSICAL_DOC,
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "UserInputService",
            ty: "UserInputService",
            doc: "The player's keyboard, mouse and controllers (same as `game:GetService(\"UserInputService\")`).",
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "ContextActionService",
            ty: "ContextActionService",
            doc: "Functions bound to inputs by priority (same as `game:GetService(\"ContextActionService\")`).",
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "RunService",
            ty: "RunService",
            doc: "The game's frame loop, with `Stepped`, `Heartbeat` and `RenderStepped` (same as `game:GetService(\"RunService\")`).",
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
            name: "CollectionService",
            ty: "CollectionService",
            doc: "Tags on Instances (same as `game:GetService(\"CollectionService\")`).",
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "SaveService",
            ty: "SaveService",
            doc: "Save slots, player data, settings and checkpoints (same as `game:GetService(\"SaveService\")`).",
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "LevelService",
            ty: "LevelService",
            doc: "The game's levels (same as `game:GetService(\"LevelService\")`).",
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "NetworkService",
            ty: "NetworkService",
            doc: "Hosting and joining games (same as `game:GetService(\"NetworkService\")`).",
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "LocalizationService",
            ty: "LocalizationService",
            doc: "The language UI text shows (same as `game:GetService(\"LocalizationService\")`).",
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "GuiService",
            ty: "GuiService",
            doc: "The player's game UI settings (same as `game:GetService(\"GuiService\")`).",
        },
    ),
    (
        ClassName::DataModel,
        Field {
            name: "UserGameSettings",
            ty: "UserGameSettings",
            doc: "This computer's graphics settings (same as `game:GetService(\"UserGameSettings\")`).",
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
        ClassName::Humanoid,
        Field {
            name: "WalkSpeed",
            ty: "number",
            doc: "Its controller's WalkSpeed.",
        },
    ),
    (
        ClassName::Humanoid,
        Field {
            name: "RunSpeed",
            ty: "number",
            doc: "Its controller's RunSpeed.",
        },
    ),
    (
        ClassName::Humanoid,
        Field {
            name: "JumpHeight",
            ty: "number",
            doc: "Its controller's JumpHeight.",
        },
    ),
    (
        ClassName::Humanoid,
        Field {
            name: "Sit",
            ty: "boolean",
            doc: "Its controller's Sit.",
        },
    ),
    (
        ClassName::Humanoid,
        Field {
            name: "PlatformStand",
            ty: "boolean",
            doc: "Its controller's PlatformStand.",
        },
    ),
    (
        ClassName::Humanoid,
        Field {
            name: "AutoRotate",
            ty: "boolean",
            doc: "Its controller's AutoRotate.",
        },
    ),
    (
        ClassName::Humanoid,
        Field {
            name: "MoveDirection",
            ty: "Vector3",
            doc: "Its controller's MoveDirection.",
        },
    ),
    (
        ClassName::Humanoid,
        Field {
            name: "Health",
            ty: "number",
            doc: "Its Health's Health; setting it fires HealthChanged (and Died at 0).",
        },
    ),
    (
        ClassName::Humanoid,
        Field {
            name: "MaxHealth",
            ty: "number",
            doc: "Its Health's MaxHealth.",
        },
    ),
    (
        ClassName::Humanoid,
        Field {
            name: "DisplayName",
            ty: "string",
            doc: "Its Health's DisplayName.",
        },
    ),
    (
        ClassName::Humanoid,
        Field {
            name: "HealthDisplayDistance",
            ty: "number",
            doc: "Its Health's HealthDisplayDistance.",
        },
    ),
    (
        ClassName::Sequence,
        Field {
            name: "TimePosition",
            ty: "number",
            doc: "Seconds from its start. Set it to jump there (the events between aren't fired).",
        },
    ),
    (
        ClassName::Sequence,
        Field {
            name: "IsPlaying",
            ty: "boolean",
            doc: "True from Play until it's paused, stopped or ends (read-only).",
        },
    ),
    (
        ClassName::Sequence,
        Field {
            name: "Length",
            ty: "number",
            doc: "Its file's duration in seconds (read-only; 0 if the file can't be read).",
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
    (
        ClassName::HingeConstraint,
        Field {
            name: "CurrentAngle",
            ty: "number",
            doc: "How far Attachment1 is turned from Attachment0 around the axis now, in degrees from -180 to 180 (read-only).",
        },
    ),
    (
        ClassName::CylindricalConstraint,
        Field {
            name: "CurrentAngle",
            ty: "number",
            doc: "How far Attachment1 is turned from Attachment0 around the axis now, in degrees from -180 to 180 (read-only).",
        },
    ),
    (
        ClassName::PrismaticConstraint,
        Field {
            name: "CurrentPosition",
            ty: "number",
            doc: "How far Attachment1 is from Attachment0 along the axis now, in studs (read-only).",
        },
    ),
    (
        ClassName::CylindricalConstraint,
        Field {
            name: "CurrentPosition",
            ty: "number",
            doc: "How far Attachment1 is from Attachment0 along the axis now, in studs (read-only).",
        },
    ),
    (
        ClassName::SpringConstraint,
        Field {
            name: "CurrentLength",
            ty: "number",
            doc: "How far apart the attachments are now, in studs (read-only).",
        },
    ),
    (
        ClassName::RopeConstraint,
        Field {
            name: "CurrentLength",
            ty: "number",
            doc: "How far apart the attachments are now, in studs (read-only).",
        },
    ),
    (
        ClassName::RodConstraint,
        Field {
            name: "CurrentLength",
            ty: "number",
            doc: "How far apart the attachments are now, in studs (read-only).",
        },
    ),
];

pub(super) const LIBRARIES: &[Library] = &[
    Library {
        global: "Instance",
        doc: "Creates Instances.",
        constants: &[],
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
        constants: &[
            Field {
                name: "zero",
                ty: "Vector3",
                doc: "(0, 0, 0).",
            },
            Field {
                name: "one",
                ty: "Vector3",
                doc: "(1, 1, 1).",
            },
            Field {
                name: "xAxis",
                ty: "Vector3",
                doc: "(1, 0, 0).",
            },
            Field {
                name: "yAxis",
                ty: "Vector3",
                doc: "(0, 1, 0): up.",
            },
            Field {
                name: "zAxis",
                ty: "Vector3",
                doc: "(0, 0, 1).",
            },
        ],
        functions: &[Function {
            name: "new",
            doc: "A new Vector3. Left-out components are 0.",
            params: &[("x?", "number"), ("y?", "number"), ("z?", "number")],
            returns: "Vector3",
        }],
    },
    Library {
        global: "Vector2",
        doc: "Creates Vector2 values.",
        constants: &[
            Field {
                name: "zero",
                ty: "Vector2",
                doc: "(0, 0).",
            },
            Field {
                name: "one",
                ty: "Vector2",
                doc: "(1, 1).",
            },
            Field {
                name: "xAxis",
                ty: "Vector2",
                doc: "(1, 0).",
            },
            Field {
                name: "yAxis",
                ty: "Vector2",
                doc: "(0, 1).",
            },
        ],
        functions: &[Function {
            name: "new",
            doc: "A new Vector2. Left-out components are 0.",
            params: &[("x?", "number"), ("y?", "number")],
            returns: "Vector2",
        }],
    },
    Library {
        global: "PhysicalProperties",
        doc: "Creates PhysicalProperties for `CustomPhysicalProperties`.",
        constants: &[],
        functions: &[Function {
            name: "new",
            doc: "A material's properties (`PhysicalProperties.new(Enum.Material.Ice)`), or your own: `PhysicalProperties.new(density, friction, elasticity, frictionWeight, elasticityWeight)`. The weights are 1 when left out; numbers out of range are brought into it.",
            params: &[
                ("densityOrMaterial", "number | EnumMaterial"),
                ("friction?", "number"),
                ("elasticity?", "number"),
                ("frictionWeight?", "number"),
                ("elasticityWeight?", "number"),
            ],
            returns: "PhysicalProperties",
        }],
    },
    Library {
        global: "Color3",
        doc: "Creates Color3 values.",
        constants: &[],
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
            Function {
                name: "fromHSV",
                doc: "A color from hue, saturation and value, each from 0 to 1. Hue goes round the color wheel from red (0) through green (1/3) and blue (2/3) back to red (1).",
                params: &[
                    ("hue", "number"),
                    ("saturation", "number"),
                    ("value", "number"),
                ],
                returns: "Color3",
            },
            Function {
                name: "fromHex",
                doc: "A color from six hex digits such as `\"#ff8000\"` or three such as `\"#f80\"`, with or without the `#`.",
                params: &[("hex", "string")],
                returns: "Color3",
            },
        ],
    },
    Library {
        global: "CFrame",
        doc: "Creates CFrame values.",
        constants: &[Field {
            name: "identity",
            ty: "CFrame",
            doc: "At the origin with no turn.",
        }],
        functions: &[
            Function {
                name: "new",
                doc: "A CFrame at a position with no turn: `CFrame.new()`, `CFrame.new(position)` or `CFrame.new(x, y, z)`.",
                params: &[
                    ("x?", "number | Vector3"),
                    ("y?", "number"),
                    ("z?", "number"),
                ],
                returns: "CFrame",
            },
            Function {
                name: "lookAt",
                doc: "A CFrame at `at` whose front (LookVector) faces `target`, with its top as close to `up` (left out: straight up) as it can be.",
                params: &[("at", "Vector3"), ("target", "Vector3"), ("up?", "Vector3")],
                returns: "CFrame",
            },
            Function {
                name: "Angles",
                doc: "A turn by these angles in radians around X, then Y, then Z, such as `CFrame.Angles(0, math.rad(90), 0)`.",
                params: &[("rx?", "number"), ("ry?", "number"), ("rz?", "number")],
                returns: "CFrame",
            },
            Function {
                name: "fromEulerAnglesYXZ",
                doc: "The turn an `Orientation` stands for, in radians: around Z, then X, then Y.",
                params: &[("rx?", "number"), ("ry?", "number"), ("rz?", "number")],
                returns: "CFrame",
            },
            Function {
                name: "fromAxisAngle",
                doc: "A turn by `angle` radians around `axis`.",
                params: &[("axis", "Vector3"), ("angle", "number")],
                returns: "CFrame",
            },
        ],
    },
    Library {
        global: "Random",
        doc: "Creates random number generators.",
        constants: &[],
        functions: &[Function {
            name: "new",
            doc: "A generator. The same `seed` gives the same numbers on every run and platform; left out, each generator is different.",
            params: &[("seed?", "number")],
            returns: "Random",
        }],
    },
    Library {
        global: "UDim2",
        doc: "Creates UI positions and sizes.",
        constants: &[],
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
        constants: &[],
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
        global: "RaycastParams",
        doc: "Creates RaycastParams values.",
        constants: &[],
        functions: &[Function {
            name: "new",
            doc: "Params that find every part scripts can query. `fields` sets some at once: `RaycastParams.new({FilterDescendantsInstances = {character}})`.",
            params: &[("fields?", "{[string]: any}")],
            returns: "RaycastParams",
        }],
    },
    Library {
        global: "OverlapParams",
        doc: "Creates OverlapParams values.",
        constants: &[],
        functions: &[Function {
            name: "new",
            doc: "Params that list every part scripts can query. `fields` sets some at once: `OverlapParams.new({MaxParts = 10})`.",
            params: &[("fields?", "{[string]: any}")],
            returns: "OverlapParams",
        }],
    },
    Library {
        global: "task",
        doc: "Pauses scripts and runs functions later, without stopping the game.",
        constants: &[],
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
            Function {
                name: "defer",
                doc: "Runs `callback` (or resumes a thread) with the given arguments at the next frame step, after this code has finished or paused.",
                params: &[("callback", "(...any) -> ()"), ("...", "any")],
                returns: "thread",
            },
            Function {
                name: "cancel",
                doc: "Stops `thread` (from `task.spawn`, `task.delay` or `task.defer`) for good: it doesn't start or resume again. It can't stop the code running now.",
                params: &[("thread", "thread")],
                returns: "()",
            },
            Function {
                name: "synchronize",
                doc: "Does nothing: every script runs on one thread, so there's nothing to wait for. For code written for parallel Luau.",
                params: &[],
                returns: "()",
            },
            Function {
                name: "desynchronize",
                doc: "Does nothing: every script runs on one thread. For code written for parallel Luau.",
                params: &[],
                returns: "()",
            },
        ],
    },
];

pub(super) const ENUM_GLOBAL_DOC: &str =
    "Every enum, such as `Enum.Material`, `Enum.PartType` and `Enum.KeyCode`.";
