//! What the tutorials say. Each one is a single task that follows a sample
//! project in `examples/`; a test checks every sample still opens and plays.
//! Text between backticks is shown in the code font.

use super::pictures::Picture;
use crate::theme::icons;

/// One task, followed step by step in a sample project.
pub(in crate::start) struct Tutorial {
    pub(in crate::start) title: &'static str,
    pub(in crate::start) icon: &'static str,
    /// One sentence for its card.
    pub(in crate::start) summary: &'static str,
    /// The sample's folder in `examples/`.
    pub(in crate::start) sample: &'static str,
    /// The sample's name, as its `axcenity.json` has it.
    pub(in crate::start) sample_name: &'static str,
    pub(in crate::start) steps: &'static [Step],
    /// API Reference entries it uses, by full name (`Part.Touched`).
    pub(in crate::start) look_up: &'static [&'static str],
}

pub(in crate::start) struct Step {
    pub(in crate::start) title: &'static str,
    pub(in crate::start) text: &'static str,
    pub(in crate::start) picture: Option<Picture>,
}

/// How many tutorials, from the first, are listed under "Start here".
pub(in crate::start) const START_HERE: usize = 4;

const HOME_TOOLS: &[(&str, &str)] = &[
    (icons::CURSOR, "Select"),
    (icons::ARROWS_OUT_CARDINAL, "Move"),
    (icons::ARROWS_OUT, "Resize"),
    (icons::ARROW_CLOCKWISE, "Rotate"),
    (icons::CUBE, "Block"),
    (icons::SPHERE, "Sphere"),
    (icons::CYLINDER, "Cylinder"),
    (icons::TRIANGLE, "Wedge"),
];

const HOME_PLAY: &[(&str, &str)] = &[
    (icons::PLAY, "Play"),
    (icons::STOP, "Stop"),
    (icons::CURSOR, "Select"),
    (icons::ARROWS_OUT_CARDINAL, "Move"),
    (icons::ARROWS_OUT, "Resize"),
    (icons::ARROW_CLOCKWISE, "Rotate"),
];

const SCRIPT_BUTTONS: &[(&str, &str)] = &[
    (icons::FILE_CODE, "Script"),
    (icons::USER, "LocalScript"),
    (icons::PACKAGE, "ModuleScript"),
];

pub(in crate::start) const TUTORIALS: &[Tutorial] = &[
    Tutorial {
        title: "Place and move a part",
        icon: icons::CUBE,
        summary: "Insert a block, then move, resize and rotate it with the handles.",
        sample: "hello-part",
        sample_name: "Hello Part",
        steps: &[
            Step {
                title: "Open the sample",
                text: "Press Open sample above. The editor opens Hello Part, a red Block on \
                       a patch of terrain, and keeps this tutorial in Help > Learn so you can \
                       follow along.",
                picture: None,
            },
            Step {
                title: "Look around",
                text: "Hold the right mouse button and move the mouse to look. While holding \
                       it, W, A, S and D fly, and E and Q go up and down. The wheel zooms, \
                       and Shift+wheel changes how fast you fly.",
                picture: None,
            },
            Step {
                title: "Insert a block",
                text: "On the Home tab, press Block in the Parts group. A 4 by 1 by 2 part \
                       appears where the camera is looking and is selected: the Explorer \
                       highlights it and Properties shows its values.",
                picture: Some(Picture::Ribbon {
                    tab: "Home",
                    buttons: HOME_TOOLS,
                    pressed: 4,
                }),
            },
            Step {
                title: "Move it",
                text: "Pick Move (1). Drag the red, green or blue arrow to slide the \
                       part along X, Y or Z, one stud at a time while Snap is on. You can \
                       also drag the part by its body to drop it onto another surface.",
                picture: Some(Picture::MoveHandles),
            },
            Step {
                title: "Resize and rotate it",
                text: "Resize (2) drags one face and keeps the opposite face still. \
                       Rotate (3) turns the part around a ring in 15° steps, and Transform \
                       (4) has all three. For exact values, type into its panel, or into \
                       `Position`, `Orientation` and `Size` under Part in Properties.",
                picture: None,
            },
            Step {
                title: "Undo and save",
                text: "Ctrl+Z undoes the last change (one drag is one step) and Ctrl+Y redoes \
                       it. Ctrl+S saves, and the `*` after the name in the title goes away. \
                       Saving changes the sample itself, so undo first if you want to keep \
                       it as it was.",
                picture: None,
            },
        ],
        look_up: &["Part", "Part.Position", "Part.Size", "Part.Anchored"],
    },
    Tutorial {
        title: "Run a script",
        icon: icons::CODE,
        summary: "Open a Lua script, write your own, and see what it prints.",
        sample: "hello-script",
        sample_name: "Hello Script",
        steps: &[
            Step {
                title: "Find the scripts",
                text: "Open the sample. Scripts that run on the server live in \
                       ServerScriptService in the Explorer. Hello Script has two: `build`, \
                       which moves the Block and makes a row of balls, and `greet`.",
                picture: Some(Picture::Tree {
                    title: "Explorer",
                    rows: &[
                        (0, icons::GLOBE_HEMISPHERE_WEST, "Workspace"),
                        (1, icons::CUBE, "Floor"),
                        (1, icons::CUBE, "Block"),
                        (0, icons::HARD_DRIVES, "ServerScriptService"),
                        (1, icons::FILE_CODE, "build"),
                        (1, icons::FILE_CODE, "greet"),
                    ],
                    selected: 5,
                }),
            },
            Step {
                title: "Open one",
                text: "Double-click `greet`. It opens in its own tab next to the Scene. You \
                       can drag the tab beside the Scene to see both at once.",
                picture: Some(Picture::Code {
                    file: "greet.server.lua",
                    code: "message = \"Hello from \" .. script.Name .. \"!\"\nprint(message)",
                }),
            },
            Step {
                title: "Make your own",
                text: "On the Script tab, press Script. The editor makes \
                       `scripts/Script.server.lua` with `print(\"Hello world!\")` in it and \
                       opens it.",
                picture: Some(Picture::Ribbon {
                    tab: "Script",
                    buttons: SCRIPT_BUTTONS,
                    pressed: 0,
                }),
            },
            Step {
                title: "Change it",
                text: "Replace the line with the code below, then press Ctrl+S. The `●` on \
                       the tab goes away once it's saved.",
                picture: Some(Picture::Code {
                    file: "Script.server.lua",
                    code: "local block = workspace.Block\nblock.Color = Color3.fromRGB(255, 200, 60)\nprint(\"Painted \" .. block.Name)",
                }),
            },
            Step {
                title: "Run it",
                text: "Scripts only run during Play, so press Play (F5). The Block turns \
                       yellow and the Console shows what each script printed. A mistake shows \
                       as a red line such as `Script.server.lua:2: ...`; click it to jump to \
                       that line. Press Stop (Shift+F5) when you're done.",
                picture: Some(Picture::Console(&[
                    "Painted Block",
                    "Built 5 balls",
                    "Hello from greet!",
                ])),
            },
        ],
        look_up: &["Instance.new", "Part.Color", "Color3.fromRGB"],
    },
    Tutorial {
        title: "Press Play and Stop",
        icon: icons::PLAY,
        summary: "Run the game in the viewport, watch it, and go back to editing.",
        sample: "hello-touched",
        sample_name: "Hello Touched",
        steps: &[
            Step {
                title: "Open the sample",
                text: "Hello Touched has a Floor, a Pad and a red Ball hanging above the pad. \
                       Its script, `pad`, waits for something to touch the pad.",
                picture: Some(Picture::Tree {
                    title: "Explorer",
                    rows: &[
                        (0, icons::GLOBE_HEMISPHERE_WEST, "Workspace"),
                        (1, icons::CUBE, "Floor"),
                        (1, icons::CUBE, "Pad"),
                        (1, icons::SPHERE, "Ball"),
                        (0, icons::HARD_DRIVES, "ServerScriptService"),
                        (1, icons::FILE_CODE, "pad"),
                    ],
                    selected: 2,
                }),
            },
            Step {
                title: "Press Play",
                text: "Press Play on the Home tab, or F5. The editor saves first, then runs \
                       the scripts and the physics in the viewport. The header shows Playing.",
                picture: Some(Picture::Ribbon {
                    tab: "Home",
                    buttons: HOME_PLAY,
                    pressed: 0,
                }),
            },
            Step {
                title: "Watch it",
                text: "The Ball falls onto the Pad. Its `Touched` event fires, so the pad \
                       changes color and glows for half a second. Three seconds in, a blue \
                       ball drops; six seconds in, the script stops listening.",
                picture: Some(Picture::Touch),
            },
            Step {
                title: "Read the Console",
                text: "Everything the script prints appears in the Console under the \
                       viewport, as it happens.",
                picture: Some(Picture::Console(&[
                    "Ball touched the pad",
                    "BlueBall touched the pad",
                    "stopped listening for touches",
                ])),
            },
            Step {
                title: "Press Stop",
                text: "Press Stop or Shift+F5. The world goes back to exactly how it was \
                       before Play, so anything the game moved or made is gone and you can \
                       press Play again for a fresh run.",
                picture: Some(Picture::Flow(&[
                    (icons::PENCIL_SIMPLE, "Edit the world"),
                    (icons::PLAY, "Play runs scripts and physics"),
                    (icons::STOP, "Stop puts the world back"),
                ])),
            },
            Step {
                title: "Try a change",
                text: "Select the Ball, raise its `Position` Y in Properties, and press Play \
                       again. It falls from higher and lands later.",
                picture: None,
            },
        ],
        look_up: &["Part.Touched", "task.wait", "Part.Material"],
    },
    Tutorial {
        title: "Open a sample",
        icon: icons::FOLDER_OPEN,
        summary: "Open any sample project, look inside it, and run it.",
        sample: "hello-physics",
        sample_name: "Hello Physics",
        steps: &[
            Step {
                title: "Where samples live",
                text: "Samples are in the engine's `examples` folder. Each one is an ordinary \
                       project folder: `axcenity.json` names it, `world.json` holds the \
                       world, and `scripts` holds its scripts.",
                picture: Some(Picture::Tree {
                    title: "Files",
                    rows: &[
                        (0, icons::FOLDER, "examples"),
                        (1, icons::FOLDER_OPEN, "hello-physics"),
                        (2, icons::FILE, "axcenity.json"),
                        (2, icons::FILE, "world.json"),
                        (2, icons::FOLDER, "scripts"),
                        (3, icons::FILE_CODE, "drop.server.lua"),
                        (3, icons::FILE_CODE, "reset.server.lua"),
                    ],
                    selected: 1,
                }),
            },
            Step {
                title: "Open it",
                text: "Press Open sample above. For any other project, press Open Project on \
                       the Projects page (or File > Open Project... in the editor) and pick \
                       its folder.",
                picture: Some(Picture::StartScreen),
            },
            Step {
                title: "Look inside",
                text: "Click a part in the viewport or the Explorer to see it in Properties. \
                       Anchored parts stay where they are; the red Dropper starts anchored, \
                       hanging in the air.",
                picture: None,
            },
            Step {
                title: "Play it",
                text: "Press Play (F5). `drop` lets go of the Dropper, freezes it in mid-air, \
                       then lets it fall onto the tower.",
                picture: Some(Picture::Console(&[
                    "released the dropper",
                    "froze the dropper in mid-air",
                    "released it again",
                ])),
            },
            Step {
                title: "Find it again",
                text: "The sample is now under Recent projects on the Projects page, so one \
                       click opens it next time. File > Close Project goes back there.",
                picture: None,
            },
            Step {
                title: "Run it without the editor",
                text: "Every sample also runs in the player. From the engine's folder, run \
                       `cargo run -p axcenity-player -- examples/hello-physics`.",
                picture: None,
            },
        ],
        look_up: &["Part.Anchored", "Part.CanCollide"],
    },
    Tutorial {
        title: "Build with blocks",
        icon: icons::PUZZLE_PIECE,
        summary: "Snap a script together from blocks and see the Lua it becomes.",
        sample: "hello-touched-blocks",
        sample_name: "Hello Touched (Blocks)",
        steps: &[
            Step {
                title: "Open the block script",
                text: "Open the sample and double-click `pad` in ServerScriptService. It's \
                       the Hello Touched game, made from blocks instead of typed Lua.",
                picture: Some(Picture::Blocks),
            },
            Step {
                title: "Add a block",
                text: "Drag a block from the palette at the side and drop it under an event \
                       block, such as when touched. It snaps into place and runs in order \
                       from the top.",
                picture: None,
            },
            Step {
                title: "See the Lua",
                text: "Press View as Lua to see the code the blocks become. Blocks are \
                       another way to write Lua, so the game runs exactly the same either way.",
                picture: Some(Picture::Code {
                    file: "pad (as Lua)",
                    code: "workspace.Pad.Touched:Connect(function(otherPart)\n    task.wait(0.5)\nend)",
                }),
            },
            Step {
                title: "Switch any script",
                text: "The bar above a Script's tab switches it between Code, Blocks and \
                       Graph (Blueprint-style nodes and wires) at any time. Lua that has no \
                       block shows as a grey Lua block or node, so nothing is lost.",
                picture: None,
            },
            Step {
                title: "Play it",
                text: "Press Play (F5). The balls land on the pad and it lights up, just like \
                       the Lua version.",
                picture: Some(Picture::Touch),
            },
        ],
        look_up: &["Part.Touched", "task.wait"],
    },
    Tutorial {
        title: "Wait and repeat",
        icon: icons::ARROWS_CLOCKWISE,
        summary: "Make things happen over time with task.wait and loops.",
        sample: "hello-tasks",
        sample_name: "Hello Tasks",
        steps: &[
            Step {
                title: "Open the sample",
                text: "Hello Tasks has a Lamp and a Spinner, and two scripts: `blink` and \
                       `spin`. Double-click `blink`.",
                picture: Some(Picture::Code {
                    file: "blink.server.lua",
                    code: "while true do\n    task.wait(1)\n    if lamp.Color == red then\n        lamp.Color = blue\n    else\n        lamp.Color = red\n    end\nend",
                }),
            },
            Step {
                title: "Waiting pauses one script",
                text: "`task.wait(1)` pauses only this script for a second. The game keeps \
                       drawing and the other scripts keep running, so a loop that waits is \
                       safe.",
                picture: None,
            },
            Step {
                title: "Play it",
                text: "Press Play (F5). The lamp switches between red and blue once a second \
                       while the spinner turns.",
                picture: Some(Picture::Console(&["blink 1", "blink 2", "blink 3"])),
            },
            Step {
                title: "A loop that never waits",
                text: "A `while true do` loop with no wait never lets the game go on. After \
                       three seconds the engine stops that script and says so in the \
                       Console; the editor and the other scripts carry on.",
                picture: None,
            },
        ],
        look_up: &["task.wait", "task.spawn", "task.delay"],
    },
    Tutorial {
        title: "Make a menu",
        icon: icons::LAYOUT,
        summary: "Put a menu on the screen and make its button do something.",
        sample: "hello-ui",
        sample_name: "Hello UI",
        steps: &[
            Step {
                title: "Open the sample",
                text: "UI lives in a ScreenGui. Hello UI has one, MainMenu, with a Menu \
                       frame holding a Title, a StartButton and a Status line.",
                picture: Some(Picture::Tree {
                    title: "Explorer",
                    rows: &[
                        (0, icons::GLOBE_HEMISPHERE_WEST, "Workspace"),
                        (1, icons::DESKTOP, "MainMenu"),
                        (2, icons::SQUARE, "Menu"),
                        (3, icons::TEXT_T, "Title"),
                        (3, icons::CURSOR_CLICK, "StartButton"),
                        (3, icons::TEXT_T, "Status"),
                    ],
                    selected: 4,
                }),
            },
            Step {
                title: "Insert UI",
                text: "The UI tab inserts a Screen, then Frames, Text and Buttons into it. \
                       Drag an element's handles in the viewport to place and size it.",
                picture: Some(Picture::Ribbon {
                    tab: "UI",
                    buttons: &[
                        (icons::DESKTOP, "Screen"),
                        (icons::SQUARE, "Frame"),
                        (icons::TEXT_T, "Text"),
                        (icons::CURSOR_CLICK, "Button"),
                        (icons::IMAGE, "Image"),
                    ],
                    pressed: 3,
                }),
            },
            Step {
                title: "Style it",
                text: "The sample's look comes from `ui.css`. Select an element and use the \
                       Style section in Properties to change its CSS.",
                picture: None,
            },
            Step {
                title: "Make the button work",
                text: "Clicks are read on the player's side, so the script is a LocalScript: \
                       `menu`, in StarterPlayerScripts. It connects to the button's \
                       `Activated` event.",
                picture: Some(Picture::Code {
                    file: "menu.client.lua",
                    code: "local menu = workspace.MainMenu.Menu\nlocal status = menu.Status\n\nmenu.StartButton.Activated:Connect(function()\n    status.Text = \"Button activated!\"\nend)",
                }),
            },
            Step {
                title: "Play it",
                text: "Press Play (F5) and click the button. The status line changes.",
                picture: Some(Picture::Menu),
            },
        ],
        look_up: &["ScreenGui", "TextButton.Activated", "TextLabel.Text"],
    },
    Tutorial {
        title: "Light a scene",
        icon: icons::LIGHTBULB,
        summary: "Add point and spot lights to parts and tune them.",
        sample: "hello-lights",
        sample_name: "Hello Lights",
        steps: &[
            Step {
                title: "Open the sample",
                text: "A light shines from the part it's in. Hello Lights has a PointLight \
                       in the Lamp, which shines every way, and a SpotLight in the Spotlight \
                       part, which shines in a cone.",
                picture: Some(Picture::Tree {
                    title: "Explorer",
                    rows: &[
                        (0, icons::GLOBE_HEMISPHERE_WEST, "Workspace"),
                        (1, icons::CUBE, "Floor"),
                        (1, icons::CUBE, "Lamp"),
                        (2, icons::LIGHTBULB, "PointLight"),
                        (1, icons::CUBE, "Spotlight"),
                        (2, icons::FLASHLIGHT, "SpotLight"),
                    ],
                    selected: 3,
                }),
            },
            Step {
                title: "Add a light",
                text: "Select a part, then press PointLight or SpotLight in the Lights & Sky \
                       group of the World tab. The light goes into the part.",
                picture: Some(Picture::Ribbon {
                    tab: "World",
                    buttons: &[
                        (icons::LIGHTBULB, "PointLight"),
                        (icons::FLASHLIGHT, "SpotLight"),
                        (icons::SUN, "Atmosphere"),
                        (icons::SPARKLE, "Particles"),
                    ],
                    pressed: 0,
                }),
            },
            Step {
                title: "Tune it",
                text: "Select the light and change its `Color`, `Brightness` and `Range` in \
                       Properties; the viewport updates as you drag.",
                picture: None,
            },
            Step {
                title: "Shadows and sky",
                text: "Select Lighting in the Explorer for shadows and their softness, the \
                       sky and bloom. Turn off `CastShadow` on a part to stop it casting a \
                       shadow.",
                picture: None,
            },
            Step {
                title: "Play it",
                text: "Press Play (F5). The script `lights` adds 120 more colored lights and \
                       animates them.",
                picture: None,
            },
        ],
        look_up: &["PointLight", "SpotLight", "PointLight.Brightness"],
    },
    Tutorial {
        title: "Play a sound",
        icon: icons::SPEAKER_HIGH,
        summary: "Add sounds, preview them, and play them from a script.",
        sample: "hello-audio",
        sample_name: "Hello Audio",
        steps: &[
            Step {
                title: "Open the sample",
                text: "Hello Audio has two Sounds in the Workspace: Music, which plays when \
                       the game starts, and Chime, which plays when the ball lands on the \
                       Pad.",
                picture: Some(Picture::Tree {
                    title: "Explorer",
                    rows: &[
                        (0, icons::GLOBE_HEMISPHERE_WEST, "Workspace"),
                        (1, icons::CUBE, "Floor"),
                        (1, icons::CUBE, "Pad"),
                        (1, icons::SPHERE, "Ball"),
                        (1, icons::SPEAKER_HIGH, "Chime"),
                        (1, icons::SPEAKER_HIGH, "Music"),
                    ],
                    selected: 4,
                }),
            },
            Step {
                title: "Preview a sound",
                text: "Select Chime. Properties shows its file, `Volume` and `Looped`; press \
                       ▶ next to the file to hear it without playing the game.",
                picture: None,
            },
            Step {
                title: "Add your own",
                text: "Select a part and press Sound on the World tab, then pick an audio \
                       file in Properties. Turn on `Playing` to start it with the game.",
                picture: None,
            },
            Step {
                title: "Play it from a script",
                text: "The script `sound` starts the music and plays the chime each time \
                       something touches the pad.",
                picture: Some(Picture::Code {
                    file: "sound.server.lua",
                    code: "local pad = workspace.Pad\nlocal chime = workspace.Chime\n\nworkspace.Music:Play()\npad.Touched:Connect(function()\n    chime:Play()\nend)",
                }),
            },
        ],
        look_up: &["Sound", "Sound.Play", "Sound.Volume"],
    },
    Tutorial {
        title: "Send a message to the server",
        icon: icons::BROADCAST,
        summary: "Read keys on the player's side and move a block on the server.",
        sample: "hello-remote",
        sample_name: "Hello Remote",
        steps: &[
            Step {
                title: "Two sides",
                text: "Scripts run on the server, which decides what really happens. \
                       LocalScripts run on each player's side, where the keyboard and the \
                       screen are. A RemoteEvent carries a message between them.",
                picture: Some(Picture::Flow(&[
                    (icons::KEYBOARD, "LocalScript reads the keys"),
                    (icons::BROADCAST, "MoveRequest:FireServer"),
                    (icons::HARD_DRIVES, "Script moves the block"),
                ])),
            },
            Step {
                title: "Open the sample",
                text: "MoveRequest is a RemoteEvent in ReplicatedStorage, which both sides \
                       can see. `keys` is a LocalScript in StarterPlayerScripts and `mover` \
                       is a Script.",
                picture: Some(Picture::Tree {
                    title: "Explorer",
                    rows: &[
                        (0, icons::PACKAGE, "ReplicatedStorage"),
                        (1, icons::BROADCAST, "MoveRequest"),
                        (0, icons::HARD_DRIVES, "ServerScriptService"),
                        (1, icons::FILE_CODE, "mover"),
                        (0, icons::USERS, "StarterPlayer"),
                        (1, icons::FOLDER, "StarterPlayerScripts"),
                        (2, icons::USER, "keys"),
                    ],
                    selected: 1,
                }),
            },
            Step {
                title: "Check what arrives",
                text: "The server's handler gets the player who sent the message first. \
                       Anything a player sends could be wrong, so `mover` checks it before \
                       moving the block.",
                picture: Some(Picture::Code {
                    file: "mover.server.lua",
                    code: "MoveRequest.OnServerEvent:Connect(function(_player, direction, seconds)\n    if type(seconds) ~= \"number\" then\n        return\n    end\n    -- move the block\nend)",
                }),
            },
            Step {
                title: "Play it",
                text: "Press Play (F5), click the viewport and hold the arrow keys. The blue \
                       block moves. To make a RemoteEvent of your own, right-click \
                       ReplicatedStorage and choose Insert > RemoteEvent.",
                picture: None,
            },
        ],
        look_up: &[
            "RemoteEvent.FireServer",
            "RemoteEvent.OnServerEvent",
            "UserInputService",
        ],
    },
];
