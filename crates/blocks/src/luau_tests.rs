//! Any Luau script as blocks: real scripts come back as the same code, as
//! blocks and not grey Lua, and editing a block changes only that code.

use std::path::{Path, PathBuf};

use axcenity_core::{ClassName, World};
use axcenity_script::ScriptRuntime;

use super::*;

/// The Controls menu from the genre templates: signals, closures, a lookup
/// table with computed keys, and if/elseif state.
const CONTROLS: &str = r#"-- The Controls menu: Menu (Escape, or Start on a controller) shows and
-- hides it. Click an action's button, then press the key or button to use
-- for it. Rebinds are SaveService settings, so they're saved in slots.
local UserInputService = game:GetService("UserInputService")
local gui = workspace:WaitForChild("Controls")
local panel = gui:WaitForChild("Panel")
local waiting = nil

local function show(row)
	row.Binding.Text = table.concat(UserInputService:GetActionInputs(row.Name), ", ")
end

for _, row in panel:GetChildren() do
	if row:FindFirstChild("Binding") then
		show(row)
		row.Binding.Activated:Connect(function()
			waiting = row
			row.Binding.Text = "Press a key or button..."
		end)
	end
end

local MOUSE_BUTTONS = {
	[Enum.UserInputType.MouseButton1] = true,
	[Enum.UserInputType.MouseButton2] = true,
	[Enum.UserInputType.MouseButton3] = true,
}

UserInputService.InputBegan:Connect(function(input, gameProcessedEvent)
	if waiting then
		local new = nil
		if input.KeyCode ~= Enum.KeyCode.Unknown then
			new = input.KeyCode
		elseif MOUSE_BUTTONS[input.UserInputType] and not gameProcessedEvent then
			new = input.UserInputType
		end
		if new then
			UserInputService:RebindAction(waiting.Name, { new })
			show(waiting)
			waiting = nil
		end
	elseif not gameProcessedEvent and UserInputService:IsActionPressed("Menu") then
		gui.Enabled = not gui.Enabled
	end
end)
"#;

fn lua_of(script: &BlockScript) -> String {
    translate(script).unwrap().source
}

fn grey_blocks(script: &BlockScript) -> Vec<String> {
    fn grey_value(value: &Expression, found: &mut Vec<String>) {
        if let Expression::Code { code } = value {
            found.push(code.clone());
        }
        for child in value.children() {
            grey_value(child, found);
        }
    }
    let mut found = Vec::new();
    script.for_each_block(&mut |block| {
        if let BlockKind::Code { code } | BlockKind::TopCode { code } = &block.kind {
            found.push(code.clone());
        }
        for value in block.kind.expressions() {
            grey_value(value, &mut found);
        }
    });
    found
}

/// The blocks of `script`, depth first.
fn kinds(script: &BlockScript) -> Vec<BlockKind> {
    let mut kinds = Vec::new();
    script.for_each_block(&mut |block| kinds.push(block.kind.clone()));
    kinds
}

#[test]
fn the_controls_menu_becomes_blocks_and_writes_itself_back_byte_for_byte() {
    let script = from_lua(CONTROLS).unwrap();
    assert_eq!(lua_of(&script), CONTROLS);
    assert_eq!(grey_blocks(&script), Vec::<String>::new());

    // The saved form gives the same script.
    let saved = serde_json::to_string(&script).unwrap();
    let read: BlockScript = serde_json::from_str(&saved).unwrap();
    assert_eq!(lua_of(&read), CONTROLS);

    let kinds = kinds(&script);
    let local = |wanted: &str| {
        kinds
            .iter()
            .any(|kind| matches!(kind, BlockKind::Local { names, .. } if names == wanted))
    };
    assert!(local("UserInputService") && local("gui") && local("waiting") && local("new"));

    // The function, the loop, and both signals.
    assert!(kinds.iter().any(|kind| matches!(
        kind,
        BlockKind::DefineFunction { name, local: true, params, .. }
            if name == "show" && params == "row"
    )));
    assert!(
        kinds
            .iter()
            .any(|kind| matches!(kind, BlockKind::GenericFor { names, .. } if names == "_, row"))
    );
    assert!(kinds.iter().any(|kind| matches!(
        kind,
        BlockKind::Connect { signal: Expression::Name { name }, method, .. }
            if name == "row.Binding.Activated" && method == "Connect"
    )));
    let top = |wanted: &str| {
        script.blocks.iter().any(|block| {
            matches!(&block.kind, BlockKind::OnSignal { signal: Expression::Name { name }, params, .. }
                if name == "UserInputService.InputBegan" && params == wanted)
        })
    };
    assert!(top("input, gameProcessedEvent"), "{:#?}", script.blocks);

    // The lookup table has its computed keys as values, one to a line.
    let table = kinds.iter().find_map(|kind| match kind {
        BlockKind::Local { names, values } if names == "MOUSE_BUTTONS" => values.first(),
        _ => None,
    });
    let Some(Expression::Table { fields, multiline }) = table else {
        panic!("{table:#?}");
    };
    assert!(*multiline);
    assert!(matches!(
        &fields[0],
        TableField::Keyed { key: Expression::Name { name }, value: Expression::Boolean { value: true } }
            if name == "Enum.UserInputType.MouseButton1"
    ));
}

/// The body of the first block of the kind `pick` finds, for changing it.
fn find_block<'a>(
    blocks: &'a mut [Block],
    pick: &impl Fn(&BlockKind) -> bool,
) -> Option<&'a mut Block> {
    for block in blocks {
        if pick(&block.kind) {
            return Some(block);
        }
        for body in block.kind.bodies_mut() {
            if let Some(found) = find_block(body, pick) {
                return Some(found);
            }
        }
    }
    None
}

#[test]
fn editing_a_block_changes_only_its_own_lua() {
    let mut script = from_lua(CONTROLS).unwrap();

    // A value in a slot deep inside a loop, a condition and a signal.
    let block = find_block(&mut script.blocks, &|kind| {
        matches!(kind, BlockKind::Assign { values, .. }
            if matches!(values.first(), Some(Expression::Text { .. })))
    })
    .unwrap();
    let BlockKind::Assign { values, .. } = &mut block.kind else {
        unreachable!();
    };
    values[0] = Expression::Text {
        value: "Press any key".to_owned(),
    };

    // A signal's parameters, and rewiring what a call is given.
    let connect = find_block(&mut script.blocks, &|kind| {
        matches!(kind, BlockKind::OnSignal { .. })
    })
    .unwrap();
    let BlockKind::OnSignal { params, .. } = &mut connect.kind else {
        unreachable!();
    };
    *params = "input, processed".to_owned();

    let expected = CONTROLS
        .replace("\"Press a key or button...\"", "\"Press any key\"")
        .replace(
            "function(input, gameProcessedEvent)",
            "function(input, processed)",
        );
    assert_eq!(lua_of(&script), expected);
}

#[test]
fn every_kind_of_luau_statement_and_value_comes_back_as_blocks() {
    let source = r##"--!strict
type Point = { x: number, y: number }

local a, b: number = 1, 2
local untyped
a, b = b, a
a += 1
b ..= "!"

@native local function typed<T>(value: T, ...: number): T
	return value
end

function Module.Sub:method(...)
	local count = select("#", ...)
	return count, ...
end

local handlers = {
	first = function(x)
		return x * 2
	end,
	[1 + 2] = "three",
	"listed",
}

while a < 10 do
	a = a + 1
	if a == 5 then
		continue
	end
	if a == 8 then
		break
	end
end

repeat
	local done = true
until done

for i = 10, 1, -1 do
	print(i)
end

for key: string, value in pairs(handlers) do
	do
		print(key, value)
	end
end

local text = `{a} and {b + 1}!`
local choice = if a > 1 then "big" elseif a < 0 then "small" else "none"
local cast = handlers :: any
local power = (a + 1) * 2 ^ -b
local joined = "a" .. "b" .. text
local fallback = untyped or 5
local length = #text + -a
local negated = not (a == b)
local called = (handlers.first or print)(1)
local indexed = handlers["first"](2).field[3]

task.delay(1, function()
	print("later") -- at the end of a line
end)

local connection = workspace.ChildAdded:Connect(function(child)
	print(child)
end)
connection:Disconnect()

return handlers
"##;
    let script = from_lua(source).unwrap();
    assert_eq!(lua_of(&script), source);
    // Only the type definition has no block: it does nothing when run.
    let grey = grey_blocks(&script);
    assert_eq!(grey, vec!["type Point = { x: number, y: number }"]);
}

#[test]
fn a_function_in_a_call_is_a_mouth_of_its_block_and_errors_find_blocks_in_it() {
    let script = from_lua("task.delay(1, function()\n    print(\"later\")\nend)\n").unwrap();
    let call = script.blocks[0].kind.bodies()[0][0].clone();
    assert!(matches!(call.kind, BlockKind::CallFunction { .. }));
    let mouths = call.kind.bodies();
    assert_eq!(mouths.len(), 1);
    let inner = mouths[0][0].id;
    let translation = translate(&script).unwrap();
    assert_eq!(translation.block_at(2), Some(inner));
    assert_eq!(translation.block_at(1), Some(call.id));

    // Its blocks are found, renumbered and checked like any others.
    let copy = call.duplicate();
    assert_ne!(copy.kind.bodies()[0][0].id, inner);
    let mut twice = script.clone();
    let BlockKind::RunScript { body } = &mut twice.blocks[0].kind else {
        panic!();
    };
    let duplicate = body[0].clone();
    body.push(duplicate);
    assert!(matches!(
        translate(&twice),
        Err(TranslateError::DuplicateId(_))
    ));
}

#[test]
fn what_no_block_can_write_the_same_stays_word_for_word() {
    // A call without brackets, numbers written another way, a raw byte.
    for source in [
        "require \"x\"\n",
        "print(0xFF, 1e3, 1_000)\n",
        "print(\"\\xFF\")\n",
        "local a = 1; local b = 2\n",
    ] {
        let script = from_lua(source).unwrap();
        assert!(
            lexically_same(source, &lua_of(&script)),
            "{source}: {}",
            lua_of(&script)
        );
    }
    let script = from_lua("print(0xFF)\n").unwrap();
    assert_eq!(lua_of(&script), "print(0xFF)\n");
    assert_eq!(grey_blocks(&script), vec!["0xFF"]);
}

fn lexically_same(first: &str, second: &str) -> bool {
    crate::lua::same_code(first, second)
}

/// Every `.lua` file under `folder`.
fn lua_files(folder: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            lua_files(&path, files);
        } else if path.extension().is_some_and(|ending| ending == "lua") {
            files.push(path);
        }
    }
}

#[test]
fn every_script_in_the_repo_becomes_blocks_that_write_the_same_code() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for folder in ["templates", "examples", "plugins"] {
        lua_files(&root.join(folder), &mut files);
    }
    assert!(files.len() > 30, "only {} scripts", files.len());

    let mut whole_scripts_grey = Vec::new();
    let (mut statements, mut grey) = (0, 0);
    for file in &files {
        let source = std::fs::read_to_string(file).unwrap().replace("\r\n", "\n");
        let script = from_lua(&source).unwrap_or_else(|err| panic!("{}: {err}", file.display()));
        let written = lua_of(&script);
        assert!(
            crate::lua::same_code(&source, &written),
            "{} changed:\n{written}",
            file.display()
        );
        assert!(
            crate::lua::keeps_comments(&source, &written),
            "{} lost a comment:\n{written}",
            file.display()
        );
        if matches!(
            script.blocks.as_slice(),
            [Block {
                kind: BlockKind::TopCode { .. },
                ..
            }]
        ) {
            whole_scripts_grey.push(file.display().to_string());
        }
        statements += kinds(&script).len();
        grey += grey_blocks(&script).len();
    }
    assert_eq!(whole_scripts_grey, Vec::<String>::new());
    assert!(
        grey * 100 <= statements,
        "{grey} grey pieces in {statements} blocks"
    );
}

#[test]
fn typed_scripts_as_blocks_still_run() {
    let source = r#"local total = 0
local function add(amount: number)
    total += amount
end
for _, amount in { 1, 2, 3 } do
    add(amount)
end
local doubled = {
    value = total * 2,
}
print(`total {total} doubled {doubled.value}`)
"#;
    let script = from_lua(source).unwrap();
    assert_eq!(grey_blocks(&script), Vec::<String>::new());
    let written = lua_of(&script);
    assert_eq!(written, source);

    let mut world = World::new();
    let lua = world.create(ClassName::Script).unwrap();
    world
        .set_parent(lua, Some(world.server_script_service()))
        .unwrap();
    let mut runtime = ScriptRuntime::new();
    runtime.start_script(&mut world, lua, "scripts/Typed.server.lua", &written);
    runtime.step(&mut world, 0.1);
    let output: Vec<String> = runtime
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(output, vec!["total 6 doubled 12"]);
}

#[test]
fn a_block_script_saved_as_grey_lua_is_refined_into_blocks() {
    // What switching the Controls script to Blocks gave before Luau itself
    // had blocks: the whole script in one grey block.
    let old = BlockScript {
        blocks: vec![Block::new(BlockKind::TopCode {
            code: CONTROLS.trim_end().to_owned(),
        })],
        graph: Graph {
            shown: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let refined = refine(&old).expect("the grey block becomes blocks");
    assert_eq!(lua_of(&refined), CONTROLS);
    assert_eq!(grey_blocks(&refined), Vec::<String>::new());
    assert!(refined.graph.shown, "the view is kept");
    assert!(refined.tabs);

    // Scripts with nothing grey, or whose grey Lua has no blocks, are left alone.
    assert!(refine(&refined).is_none());
    let typed = from_lua("type Point = { x: number }\n").unwrap();
    assert!(refine(&typed).is_none());
}
