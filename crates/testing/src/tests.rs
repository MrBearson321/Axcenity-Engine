use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::*;

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

/// A copy of the example `name` in a fresh temporary folder, for a test
/// that changes it.
fn copy_of(name: &str, test: &str) -> PathBuf {
    let folder = std::env::temp_dir()
        .join("axcenity-testing-tests")
        .join(format!("{test}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    copy_folder(&example(name), &folder);
    folder
}

fn copy_folder(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_folder(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Every file under `folder` and its bytes.
fn files_in(folder: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut stack = vec![folder.to_owned()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.insert(path.clone(), fs::read(path).unwrap());
            }
        }
    }
    files
}

fn run_all(folder: &Path) -> Vec<TestResult> {
    Runner::open(folder)
        .unwrap()
        .run(&Selection::All, &mut |_| {})
}

fn failure_of<'a>(results: &'a [TestResult], title: &str) -> Option<&'a str> {
    let result = results
        .iter()
        .find(|result| result.title() == title)
        .unwrap_or_else(|| panic!("no result for {title}: {results:#?}"));
    result
        .failure
        .as_ref()
        .map(|failure| failure.message.as_str())
}

fn write_spec(folder: &Path, name: &str, source: &str) {
    let tests = folder.join(TESTS_FOLDER);
    fs::create_dir_all(&tests).unwrap();
    fs::write(tests.join(name), source).unwrap();
}

#[test]
fn hello_character_walks_more_than_five_studs_while_w_is_held_for_a_second() {
    let results = run_all(&example("hello-character"));
    let titles: Vec<String> = results.iter().map(TestResult::title).collect();
    assert_eq!(
        titles,
        [
            "Character > walks more than 5 studs while W is held for a second",
            "Character > stands still on the floor without input",
            "Character > runs while Shift is held",
        ]
    );
    for result in &results {
        assert!(result.passed(), "{result:#?}");
    }
    // Held for one second of 60 frames.
    assert!(
        (results[0].seconds - 1.0).abs() < 1e-9,
        "{}",
        results[0].seconds
    );
}

#[test]
fn a_walk_speed_of_zero_fails_the_walk_with_its_line_and_values() {
    let folder = copy_of("hello-character", "walk-speed");
    let world = folder.join("world.json");
    let text = fs::read_to_string(&world).unwrap();
    fs::write(
        &world,
        text.replacen("\"WalkSpeed\": 16", "\"WalkSpeed\": 0", 1),
    )
    .unwrap();

    let results = run_all(&folder);
    let walk = &results[0];
    let failure = walk.failure.as_ref().expect("the walk fails");
    assert_eq!(
        failure.message,
        "tests/walk.spec.lua:14: expected 0 to be greater than 5"
    );
    assert_eq!(
        failure.location,
        Some(("tests/walk.spec.lua".to_owned(), 14))
    );
    // The others still pass.
    assert!(results[1..].iter().all(TestResult::passed), "{results:#?}");
    let _ = fs::remove_dir_all(folder);
}

#[test]
fn tests_leave_the_project_files_alone_even_when_they_save_and_destroy() {
    let folder = copy_of("hello-character", "files");
    write_spec(
        &folder,
        "saves.spec.lua",
        r#"
describe("Saving", function()
    it("saves a slot and destroys the floor", function()
        local SaveService = game:GetService("SaveService")
        SaveService:SetPlayerData("coins", 5)
        SaveService:SaveSlot(1)
        expect(SaveService:HasSlot(1)).toBe(true)
        workspace.Floor:Destroy()
        Test.step(0.1)
        expect(workspace:FindFirstChild("Floor")).never.toBeTruthy()
    end)

    it("finds the floor again in a fresh copy", function()
        expect(Test.find("Workspace.Floor")).toBeTruthy()
    end)
end)
"#,
    );
    let before = files_in(&folder);
    let results = run_all(&folder);
    assert!(results.iter().all(TestResult::passed), "{results:#?}");
    assert_eq!(results.len(), 5);
    assert!(before == files_in(&folder), "the project's files changed");
    let _ = fs::remove_dir_all(folder);
}

#[test]
fn a_test_that_errors_or_a_file_that_does_not_compile_does_not_stop_the_others() {
    let folder = copy_of("hello-character", "errors");
    write_spec(
        &folder,
        "a_broken.spec.lua",
        "describe(\"Broken\", function(\n",
    );
    write_spec(
        &folder,
        "b_errors.spec.lua",
        r#"
it("errors", function()
    local missing = nil
    print(missing.Name)
end)

it("passes after it", function()
    expect(1 + 1).toBe(2)
end)

afterEach(function()
    print("cleaned up")
end)
"#,
    );
    let results = run_all(&folder);
    let broken = &results[0];
    assert_eq!(broken.file, "tests/a_broken.spec.lua");
    assert!(broken.name.is_empty());
    let message = &broken.failure.as_ref().unwrap().message;
    assert!(message.starts_with("tests/a_broken.spec.lua:"), "{message}");

    let errors = failure_of(&results, "errors").unwrap();
    assert!(errors.starts_with("tests/b_errors.spec.lua:4:"), "{errors}");
    assert_eq!(failure_of(&results, "passes after it"), None);
    // `afterEach` ran after the error too.
    let errored = results.iter().find(|r| r.title() == "errors").unwrap();
    assert!(errored.output.contains(&"cleaned up".to_owned()));
    // And the walk tests still ran.
    assert_eq!(results.len(), 1 + 2 + 3);
    let _ = fs::remove_dir_all(folder);
}

#[test]
fn matchers_pass_and_fail_with_what_they_compared() {
    let folder = copy_of("hello-character", "matchers");
    write_spec(
        &folder,
        "matchers.spec.lua",
        r#"
describe("Matchers", function()
    it("pass", function()
        expect({ 1, { a = "b" } }).toEqual({ 1, { a = "b" } })
        expect(Vector3.new(1, 2, 3)).toBeNear(Vector3.new(1, 2, 3.05), 0.1)
        expect(0.1 + 0.2).toBeNear(0.3, 1e-9)
        expect(function() error("boom") end).toThrow("boom")
        expect(function() end).never.toThrow()
        expect("a"):toBe("a")
        expect(nil).never.toBeTruthy()
        expect(3).toBeLessThan(4)
        workspace.Floor:SetAttribute("Kind", "grass")
        expect(workspace.Floor).toHaveAttribute("Kind")
        expect(workspace.Floor).toHaveAttribute("Kind", "grass")
        expect(workspace.Floor).never.toHaveAttribute("Missing")
    end)

    it("toBe", function() expect(1).toBe(2) end)
    it("never", function() expect(1).never.toBe(1) end)
    it("toEqual", function() expect({ 1 }).toEqual({ 2 }) end)
    it("toBeNear", function() expect(1).toBeNear(2, 0.5) end)
    it("toThrow", function() expect(function() end).toThrow() end)
    it("toThrow message", function()
        expect(function() error("other", 0) end).toThrow("boom")
    end)
    it("toHaveAttribute", function()
        expect(workspace.Floor).toHaveAttribute("Kind", 1)
    end)
    it("wait", function() Test.wait(function() return false end, 0.25) end)
    it("press", function() Test.press("NotAKey") end)
end)
"#,
    );
    let results = run_all(&folder);
    let failure = |name: &str| failure_of(&results, &format!("Matchers > {name}"));
    assert_eq!(failure("pass"), None);
    assert_eq!(
        failure("toBe"),
        Some("tests/matchers.spec.lua:18: expected 1 to be 2")
    );
    assert_eq!(
        failure("never"),
        Some("tests/matchers.spec.lua:19: expected 1 not to be 1")
    );
    assert!(
        failure("toEqual")
            .unwrap()
            .contains(":20: expected table: ")
    );
    assert_eq!(
        failure("toBeNear"),
        Some("tests/matchers.spec.lua:21: expected 1 to be within 0.5 of 2, but it is 1 away")
    );
    assert_eq!(
        failure("toThrow"),
        Some("tests/matchers.spec.lua:22: expected the function to throw, but it didn't throw")
    );
    assert_eq!(
        failure("toThrow message"),
        Some(
            "tests/matchers.spec.lua:24: expected the function to throw an error containing \
             \"boom\", but it threw \"other\""
        )
    );
    assert!(
        failure("toHaveAttribute")
            .unwrap()
            .ends_with("'s attribute \"Kind\" to be 1, but it is nil"),
        "{:?}",
        failure("toHaveAttribute")
    );
    assert_eq!(
        failure("wait"),
        Some("tests/matchers.spec.lua:29: timed out after 0.25 seconds waiting for the condition")
    );
    assert_eq!(
        failure("press"),
        Some("`NotAKey` is not a key, a mouse button or an input action")
    );
    let _ = fs::remove_dir_all(folder);
}

#[test]
fn hooks_run_around_each_test_outermost_first_and_names_nest() {
    let folder = copy_of("hello-character", "hooks");
    write_spec(
        &folder,
        "hooks.spec.lua",
        r#"
local order = {}
beforeEach(function() table.insert(order, "outer before") end)
afterEach(function() print(table.concat(order, ", ")) end)

describe("Outer", function()
    beforeEach(function() table.insert(order, "inner before") end)
    afterEach(function() table.insert(order, "inner after") end)

    describe("Inner", function()
        it("runs", function() table.insert(order, "test") end)
    end)
end)
"#,
    );
    let mut runner = Runner::open(&folder).unwrap();
    let tests = runner.collect("tests/hooks.spec.lua").unwrap();
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].name, ["Outer", "Inner", "runs"]);
    assert_eq!(tests[0].line, 11);
    let result = runner.run_test(&tests[0]);
    assert!(result.passed(), "{result:#?}");
    assert!(
        result
            .output
            .contains(&"outer before, inner before, test, inner after".to_owned()),
        "{:?}",
        result.output
    );
    let _ = fs::remove_dir_all(folder);
}

#[test]
fn clicking_a_button_and_typing_reach_local_scripts() {
    let folder = copy_of("hello-buttons", "click");
    fs::write(
        folder.join("scripts/typed.client.lua"),
        r#"
local box = Instance.new("TextBox")
box.Name = "Name"
box.Parent = workspace.Menu
box.FocusLost:Connect(function()
    workspace.Menu.Status.Text = "Hello " .. box.Text
end)
"#,
    )
    .unwrap();
    write_spec(
        &folder,
        "menu.spec.lua",
        r#"
describe("Menu", function()
    it("plays when Play is clicked", function()
        Test.click(Test.find("Workspace.Menu.Play"))
        expect(Test.find("Workspace.Menu.Status").Text).toBe("Playing!")
    end)

    it("ignores the locked button", function()
        local status = Test.find("Workspace.Menu.Status")
        local before = status.Text
        Test.click(Test.find("Workspace/Menu/Locked"))
        expect(status.Text).toBe(before)
    end)

    it("greets what was typed", function()
        Test.type(Test.find("Workspace.Menu.Name"), "Ada")
        expect(Test.find("Workspace.Menu.Status").Text).toBe("Hello Ada")
    end)
end)
"#,
    );
    let results = run_all(&folder);
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(TestResult::passed), "{results:#?}");
    let _ = fs::remove_dir_all(folder);
}

#[test]
fn test_level_runs_the_tests_after_it_in_that_level() {
    let folder = copy_of("hello-levels", "levels");
    write_spec(
        &folder,
        "levels.spec.lua",
        r#"
it("starts in the start level", function()
    expect(Test.find("Workspace.Home")).toBeTruthy()
    expect(game:GetService("LevelService").CurrentLevel).toBe("Meadow")
end)

describe("Castle", function()
    Test.level("Castle")
    it("has its gate", function()
        expect(Test.find("Workspace.Gate")).toBeTruthy()
        expect(Test.find("Workspace.Home")).never.toBeTruthy()
        expect(game:GetService("LevelService").CurrentLevel).toBe("Castle")
    end)
end)

describe("Missing", function()
    Test.level("Nowhere")
    it("fails", function() end)
end)
"#,
    );
    let results = run_all(&folder);
    assert_eq!(failure_of(&results, "starts in the start level"), None);
    assert_eq!(failure_of(&results, "Castle > has its gate"), None);
    assert_eq!(
        failure_of(&results, "Missing > fails"),
        Some("the project has no level named \"Nowhere\"")
    );
    let _ = fs::remove_dir_all(folder);
}

#[test]
fn run_one_and_run_file_pick_their_tests() {
    let folder = example("hello-character");
    let mut runner = Runner::open(&folder).unwrap();
    let mut found = Vec::new();
    let one = runner.run(
        &Selection::One {
            file: "tests/walk.spec.lua".to_owned(),
            index: 2,
        },
        &mut |event| {
            if let RunEvent::Found { tests, .. } = event {
                found = tests;
            }
        },
    );
    assert_eq!(found.len(), 3);
    assert_eq!(one.len(), 1);
    assert_eq!(
        one[0].title(),
        "Character > stands still on the floor without input"
    );
    let file = runner.run(
        &Selection::File("tests/walk.spec.lua".to_owned()),
        &mut |_| {},
    );
    assert_eq!(file.len(), 3);
}

#[test]
fn exported_games_never_include_tests() {
    let folder = example("hello-character");
    assert!(!spec_files(&folder).is_empty());
    let bytes = axcenity_project::pack_game(&folder).unwrap();
    let unpacked = std::env::temp_dir()
        .join("axcenity-testing-tests")
        .join(format!("export-{}", std::process::id()));
    let _ = fs::remove_dir_all(&unpacked);
    axcenity_project::unpack_game(&bytes, &unpacked).unwrap();
    assert!(unpacked.join("world.json").is_file());
    assert!(!unpacked.join(TESTS_FOLDER).exists());
    assert!(spec_files(&unpacked).is_empty());
    let _ = fs::remove_dir_all(unpacked);
}

#[test]
fn junit_escapes_names_and_messages() {
    let result = TestResult {
        file: "tests/a.spec.lua".to_owned(),
        index: 1,
        name: vec!["<b> & \"c\"".to_owned()],
        line: 2,
        failure: Some(Failure::new("tests/a.spec.lua:2: expected \"<\" to be '>'")),
        output: Vec::new(),
        seconds: 0.5,
    };
    let xml = junit::junit_xml("Game", &[result]);
    assert!(
        xml.contains("name=\"&lt;b&gt; &amp; &quot;c&quot;\""),
        "{xml}"
    );
    assert!(
        xml.contains("expected &quot;&lt;&quot; to be &apos;&gt;&apos;"),
        "{xml}"
    );
}

#[test]
fn locations_come_from_the_start_of_messages() {
    assert_eq!(
        Failure::new("tests/x.spec.lua:12: nope").location,
        Some(("tests/x.spec.lua".to_owned(), 12))
    );
    assert_eq!(Failure::new("timed out").location, None);
    assert!(is_spec_file("tests/ui/menu.spec.lua"));
    assert!(!is_spec_file("scripts/menu.spec.lua"));
}
