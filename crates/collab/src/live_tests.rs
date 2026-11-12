//! Live documents (C7): copies of one text merge to the same text whatever
//! order their changes arrive in, typing at once loses nothing, and a bad
//! update is refused whole.

use crate::live::{LiveDoc, LiveError, UPDATE_MOST, is_live};

/// A member's copy, caught up with the host's.
fn joined(host: &LiveDoc) -> LiveDoc {
    let mut copy = LiveDoc::new();
    copy.apply(&host.update_since(&copy.state_vector()))
        .unwrap();
    copy
}

#[test]
fn a_member_takes_the_hosts_text_once() {
    let host = LiveDoc::with_text("print('hi')\n");
    let copy = joined(&host);
    assert_eq!(copy.text(), "print('hi')\n");
    // Taking it again changes nothing.
    let mut again = joined(&host);
    again.apply(&host.update_since(&[])).unwrap();
    assert_eq!(again.text(), "print('hi')\n");
    assert!(!host.is_ahead_of(&copy.state_vector()));
}

#[test]
fn two_people_typing_at_once_both_keep_their_text() {
    let mut host = LiveDoc::with_text("local a = 1\nlocal b = 2\n");
    let mut ana = joined(&host);
    let mut ben = joined(&host);
    let from_ana = ana.set_text("local a = 10\nlocal b = 2\n").unwrap();
    let from_ben = ben
        .set_text("local a = 1\nlocal b = 2\nprint(a + b)\n")
        .unwrap();
    // The host takes them in either order and passes each on.
    host.apply(&from_ben).unwrap();
    host.apply(&from_ana).unwrap();
    ana.apply(&from_ben).unwrap();
    ben.apply(&from_ana).unwrap();
    let merged = "local a = 10\nlocal b = 2\nprint(a + b)\n";
    assert_eq!(host.text(), merged);
    assert_eq!(ana.text(), merged);
    assert_eq!(ben.text(), merged);
}

#[test]
fn typing_offline_merges_when_back() {
    let mut host = LiveDoc::with_text("x\n");
    let mut away = joined(&host);
    let mut here = joined(&host);
    // Several edits each while apart.
    for line in ["a\n", "b\n", "c\n"] {
        let text = away.text() + line;
        away.set_text(&text);
    }
    for line in ["1\n", "2\n"] {
        let text = format!("{line}{}", here.text());
        let update = here.set_text(&text).unwrap();
        host.apply(&update).unwrap();
    }
    // Back: each side sends what the other lacks.
    let to_host = away.update_since(&host.state_vector());
    let to_away = host.update_since(&away.state_vector());
    host.apply(&to_host).unwrap();
    away.apply(&to_away).unwrap();
    assert_eq!(host.text(), "2\n1\nx\na\nb\nc\n");
    assert_eq!(away.text(), host.text());
}

#[test]
fn text_with_accents_and_emoji_changes_on_character_boundaries() {
    let mut host = LiveDoc::with_text("-- café ☕\n");
    let mut copy = joined(&host);
    let update = copy.set_text("-- cafés ☕☕\n").unwrap();
    host.apply(&update).unwrap();
    assert_eq!(host.text(), "-- cafés ☕☕\n");
    let update = copy.set_text("-- ☕\n").unwrap();
    host.apply(&update).unwrap();
    assert_eq!(host.text(), "-- ☕\n");
}

#[test]
fn a_bad_update_is_refused_and_changes_nothing() {
    let mut host = LiveDoc::with_text("keep\n");
    assert!(matches!(
        host.apply(&[0xff, 0x13, 0x07]),
        Err(LiveError::Malformed(_))
    ));
    assert_eq!(
        host.apply(&vec![0; UPDATE_MOST + 1]),
        Err(LiveError::TooBig(UPDATE_MOST + 1))
    );
    assert_eq!(host.text(), "keep\n");
}

#[test]
fn only_lua_scripts_and_stylesheets_are_live() {
    assert!(is_live("scripts/door.server.lua"));
    assert!(is_live("ui/menu.css"));
    assert!(!is_live("scripts/door.blocks.json"));
    assert!(!is_live("assets/materials/brick.material.json"));
}

/// Random typing on three copies with updates delivered late, in any
/// order and some twice: every copy ends with the same text.
#[test]
fn random_typing_with_late_and_repeated_updates_converges() {
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = move |below: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % below as u64) as usize
    };
    for _ in 0..40 {
        let host = LiveDoc::with_text("function f()\nend\n");
        let mut copies: Vec<LiveDoc> = (0..3).map(|_| joined(&host)).collect();
        let mut sent: Vec<Vec<u8>> = Vec::new();
        for _ in 0..30 {
            let who = next(3);
            let text = copies[who].text();
            let chars: Vec<char> = text.chars().collect();
            let at = next(chars.len() + 1);
            let mut new: String = chars[..at].iter().collect();
            if next(3) == 0 && at < chars.len() {
                new.extend(chars[at + 1..].iter());
            } else {
                new.push(['a', 'é', '☕', '\n'][next(4)]);
                new.extend(chars[at..].iter());
            }
            if let Some(update) = copies[who].set_text(&new) {
                sent.push(update);
            }
            // Some updates arrive now, at one other copy.
            if next(2) == 0 && !sent.is_empty() {
                let update = &sent[next(sent.len())];
                copies[next(3)].apply(update).unwrap();
            }
        }
        // Everything arrives everywhere in the end, in a shuffled order.
        for copy in &mut copies {
            for _ in 0..sent.len() {
                let update = &sent[next(sent.len())];
                copy.apply(update).unwrap();
            }
            for update in &sent {
                copy.apply(update).unwrap();
            }
        }
        let text = copies[0].text();
        assert!(copies.iter().all(|c| c.text() == text));
    }
}

#[test]
fn undo_takes_back_only_your_own_typing() {
    let mut host = LiveDoc::with_text("a\n");
    let mut mine = joined(&host);
    mine.track_own_edits();
    let mut theirs = joined(&host);
    let typed = mine.set_text("a\nmine\n").unwrap();
    host.apply(&typed).unwrap();
    theirs.apply(&typed).unwrap();
    let other = theirs.set_text("theirs\na\nmine\n").unwrap();
    host.apply(&other).unwrap();
    mine.apply(&other).unwrap();

    // Undo removes this member's line and keeps the other's.
    let undone = mine.undo().expect("something to undo");
    assert_eq!(mine.text(), "theirs\na\n");
    host.apply(&undone).unwrap();
    assert_eq!(host.text(), "theirs\na\n");
    // Others' changes aren't this member's to undo.
    assert_eq!(mine.undo(), None);

    let redone = mine.redo().expect("something to redo");
    host.apply(&redone).unwrap();
    assert_eq!(host.text(), "theirs\na\nmine\n");
    assert_eq!(mine.text(), host.text());
}

/// Typing in a 5,000-line script: what one keystroke costs on the typing
/// copy and on another (the plan's performance check). Run with
/// `--ignored --nocapture`.
#[test]
#[ignore]
fn typing_in_a_five_thousand_line_script() {
    let mut text = String::new();
    for i in 0..5000 {
        text.push_str(&format!(
            "local value{i} = compute({i}, \"some text\") -- note\n"
        ));
    }
    let host = LiveDoc::with_text(&text);
    let mut mine = joined(&host);
    mine.track_own_edits();
    let mut theirs = joined(&host);
    let middle = text.len() / 2;
    let keys = 200;
    let mut typed = 0.0;
    let mut applied = 0.0;
    let mut size = 0;
    for k in 0..keys {
        text.insert(middle + k, 'x');
        let start = std::time::Instant::now();
        let update = mine.set_text(&text).unwrap();
        typed += start.elapsed().as_secs_f64();
        size += update.len();
        let start = std::time::Instant::now();
        theirs.apply(&update).unwrap();
        applied += start.elapsed().as_secs_f64();
    }
    assert_eq!(theirs.text(), text);
    let start = std::time::Instant::now();
    for _ in 0..20 {
        std::hint::black_box(mine.text());
    }
    println!(
        "reading the text: {:.3} ms",
        start.elapsed().as_secs_f64() * 1000.0 / 20.0
    );
    println!(
        "{} bytes: a keystroke {:.3} ms to make, {:.3} ms to apply elsewhere, {} bytes sent; whole state {} KiB",
        text.len(),
        typed * 1000.0 / keys as f64,
        applied * 1000.0 / keys as f64,
        size / keys,
        mine.state().len() / 1024
    );
}

#[test]
fn a_cursor_stays_on_its_text_while_others_type_before_it() {
    let host = LiveDoc::with_text("local a = 1\nlocal b = 2\n");
    let mut mine = joined(&host);
    let mut theirs = joined(&host);
    // Their cursor at the start of line 2.
    let cursor = theirs.sticky(12).unwrap();
    assert_eq!(mine.resolve(&cursor), Some(12));
    let update = mine.set_text("-- é☕\nlocal a = 1\nlocal b = 2\n").unwrap();
    theirs.apply(&update).unwrap();
    assert_eq!(mine.resolve(&cursor), Some(18));
    assert_eq!(theirs.resolve(&cursor), Some(18));
    // The end of the text is a place too; garbage isn't.
    let length = theirs.text().chars().count();
    let end = theirs.sticky(length).expect("the end");
    let update = theirs.set_text(&format!("{}x", theirs.text())).unwrap();
    mine.apply(&update).unwrap();
    assert!(
        mine.resolve(&end).is_some_and(|at| at >= length),
        "{:?}",
        mine.resolve(&end)
    );
    assert_eq!(mine.resolve(&[0xff, 0xff]), None);
}
