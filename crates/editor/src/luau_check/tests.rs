use super::*;

#[test]
fn gnu_lines_are_read_with_their_ranges_kinds_and_long_messages() {
    let printed = "\
script.lua:2.1-2.22: TypeError: Type 'string' could not be converted into 'number'
script.lua:4.7-4.20: TypeError: Unknown global 'undefinedThing'
C:\\Users\\me\\x.lua:9.3-9.5: TypeError: Type 'string' could not be converted into 'number'
caused by:
  more detail here
script.lua:2.7-2.7: LocalUnused: Variable 'n' is never used; prefix with '_' to silence
";
    let found = parse(printed);
    assert_eq!(found.len(), 4);
    assert_eq!(
        found[1],
        Diagnostic {
            line: 4,
            column: 7,
            end_line: 4,
            end_column: 20,
            severity: Severity::Error,
            code: "TypeError".to_owned(),
            message: "Unknown global 'undefinedThing'".to_owned(),
        }
    );
    assert_eq!(found[2].line, 9);
    assert!(found[2].message.ends_with("caused by:\n  more detail here"));
    assert_eq!(found[3].severity, Severity::Warning);
    assert_eq!(found[3].code, "LocalUnused");
    assert!(parse("Failed to load definitions\n").is_empty());
}

#[test]
fn results_count_only_for_the_text_they_were_found_in() {
    let mut results = Results::default();
    let old = text_hash("print(x)");
    results.set(
        "scripts/a.server.lua",
        old,
        parse("script.lua:1.7-1.7: TypeError: Unknown global 'x'"),
    );
    assert_eq!(results.get("scripts/a.server.lua", old).unwrap().len(), 1);
    assert!(
        results
            .get("scripts/a.server.lua", text_hash("print(y)"))
            .is_none()
    );
    assert!(results.get("scripts/b.server.lua", old).is_none());
}

#[test]
fn the_newest_luau_extension_in_vs_code_is_found() {
    let extensions =
        std::env::temp_dir().join(format!("axcenity-luau-extensions-{}", std::process::id()));
    let server = if cfg!(windows) {
        "server.exe"
    } else {
        "server"
    };
    for version in ["1.9.0", "1.53.0", "1.10.2"] {
        let bin = extensions
            .join(format!("johnnymorganz.luau-lsp-{version}"))
            .join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join(server), "").unwrap();
    }
    std::fs::create_dir_all(extensions.join("someone.else-2.0.0/bin")).unwrap();
    let found = vs_code_server(&extensions).unwrap();
    assert!(
        found.to_string_lossy().contains("luau-lsp-1.53.0"),
        "{found:?}"
    );
    std::fs::remove_dir_all(&extensions).unwrap();
}

/// With luau-lsp installed (`AXCENITY_LUAU_LSP`, the PATH or VS Code), the
/// engine's definitions load and a script's errors come back on the right
/// lines. Without it, there's nothing to check.
#[test]
fn luau_lsp_checks_a_script_against_the_engine_api() {
    let Some((program, _)) = find() else {
        eprintln!("skipped: no luau-lsp");
        return;
    };
    let checker = Checker::start(program).unwrap();
    let text = "\
local part = Instance.new(\"Part\")
part.Anchored = true
local n: number = \"hi\"
print(undefinedThing)
local v = Vector3.new(1, 2, 3)
print(v.Magnitude + n)
game:GetService(\"Players\").PlayerAdded:Connect(function(player) print(player.Name) end)
";
    let key = checker.check_text("scripts/a.server.lua", text);
    let checked = checker.wait(Duration::from_secs(30)).unwrap();
    assert_eq!(checked.path, "scripts/a.server.lua");
    assert_eq!(checked.key, key);
    let found = checked.result.unwrap();
    let errors: Vec<(usize, &str)> = found
        .iter()
        .filter(|found| found.severity == Severity::Error)
        .map(|found| (found.line, found.message.as_str()))
        .collect();
    assert_eq!(errors.len(), 2, "{found:#?}");
    assert_eq!(errors[0].0, 3);
    // luau-lsp 1.70 adds "; consider assigning to it first".
    assert_eq!(errors[1].0, 4);
    assert!(
        errors[1].1.starts_with("Unknown global 'undefinedThing'"),
        "{}",
        errors[1].1
    );

    // A clean script: the engine's API is all known.
    checker.check_text(
        "scripts/a.server.lua",
        "workspace.Gravity = 50\nprint(math.floor(2.5))\n",
    );
    let checked = checker.wait(Duration::from_secs(30)).unwrap();
    assert_eq!(checked.result.unwrap(), Vec::new());
}

/// The variable wins; then the shipped server, then a downloaded one
/// (both the pinned version), before the PATH's and VS Code's.
#[test]
fn the_shipped_luau_lsp_is_used_before_the_paths() {
    let folder = std::env::temp_dir().join(format!("axcenity-lsp-find-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let name = crate::luau_lsp::program_name();
    let make = |relative: &str| {
        let program = folder.join(relative).join(name);
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        std::fs::write(&program, "").unwrap();
        program
    };
    let on_path = make("bin");
    let bundled = make("editor/tools");
    let downloaded = make("data/Tools/luau-lsp-1");
    let mut places = Places {
        variable: None,
        bundled: Some(bundled.clone()),
        downloaded: Some(downloaded.clone()),
        path: Some(std::env::join_paths([folder.join("bin")]).unwrap()),
        vs_code_extensions: None,
    };
    assert_eq!(find_in(&places), Some((bundled.clone(), Source::Bundled)));
    std::fs::remove_file(&bundled).unwrap();
    assert_eq!(
        find_in(&places),
        Some((downloaded.clone(), Source::Downloaded))
    );
    std::fs::remove_file(&downloaded).unwrap();
    assert_eq!(find_in(&places), Some((on_path.clone(), Source::Path)));
    // A variable naming nothing finds nothing, rather than another one.
    places.variable = Some(folder.join("missing"));
    assert_eq!(find_in(&places), None);
    places.variable = Some(on_path.clone());
    assert_eq!(find_in(&places), Some((on_path, Source::Variable)));
    std::fs::remove_dir_all(&folder).unwrap();
}
