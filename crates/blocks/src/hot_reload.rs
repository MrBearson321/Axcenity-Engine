//! Getting a script ready to hot reload with its state kept (unified Phase
//! 89, the way s&box hotloads game code).
//!
//! [`wrap`] rewrites each top-level `local` so that, when the script runs
//! again after a save, the local can take the value it had in the old run
//! instead of running its initializer:
//!
//! ```lua
//! local count = 0
//! -- becomes, on the same line:
//! local count; if __axcenity_kept({"count"}, "1f2e…") then count = __axcenity_values() else count = 0 end; __axcenity_watch({"count"}, "1f2e…", function() return count end)
//! ```
//!
//! `__axcenity_watch` lets the runtime read the local's value before the
//! script reloads, and `__axcenity_kept` says whether a kept value is
//! there. The string is a hash of the initializer as written, so a local
//! whose initializer was edited takes the new value. Everything stays on
//! the line it was on, so errors and breakpoints point at the same lines.
//! The runtime side is `axcenity_script::hot_reload`.
//!
//! A local isn't wrapped (and so always runs its initializer) when:
//! - its line ends in a `-- @reset` comment;
//! - its initializer uses one of the names it declares (`local print =
//!   print`), which would mean something else once declared first;
//! - its initializer uses `...`, which a script's top level never has
//!   anyway.

use std::fmt::Write as _;

use crate::lua::{self, ExprKind, StatKind, TableField};

/// The names the runtime puts in a script's environment for [`wrap`]'s
/// code. Keep in step with `axcenity_script::hot_reload`.
pub const KEPT: &str = "__axcenity_kept";
pub const VALUES: &str = "__axcenity_values";
pub const WATCH: &str = "__axcenity_watch";

/// `source` with its top-level locals wrapped (see the module docs), or
/// `None` when there's nothing to wrap or it isn't Luau this reader reads.
/// The result runs as `source` does on a first run.
pub fn wrap(source: &str) -> Option<String> {
    let stats = lua::parse(source).ok()?;
    let mut seen: std::collections::HashMap<String, usize> = Default::default();
    // (where, what) edits, applied back to front.
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for stat in &stats {
        let StatKind::Local { names, values } = &stat.kind else {
            continue;
        };
        let bare: Vec<&str> = names.iter().map(|name| bare_name(name)).collect();
        // Every local gets its key, wrapped or not, so `x` and a later
        // `x` (shadowing it) are told apart however the code changes.
        let keys: Vec<String> = bare
            .iter()
            .map(|name| {
                let count = seen.entry((*name).to_owned()).or_default();
                *count += 1;
                match *count {
                    1 => (*name).to_owned(),
                    n => format!("{name}#{n}"),
                }
            })
            .collect();
        if resets(source, stat.span.end) {
            continue;
        }
        let init = values
            .first()
            .map(|first| &source[first.span.start..values.last().unwrap().span.end]);
        if let Some(init) = init
            && (bare.iter().any(|name| lua::mentions(init, name)) || values.iter().any(uses_vararg))
        {
            continue;
        }
        let hash = format!("{:016x}", text_hash(init.unwrap_or("")));
        let key_list = keys
            .iter()
            .map(|key| format!("{key:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        let targets = bare.join(", ");
        let declared_end = values.first().map_or(stat.span.end, |_| {
            // The names (with their types) end before `=`.
            source[..values[0].span.start]
                .trim_end()
                .strip_suffix('=')
                .map_or(stat.span.end, |before| before.trim_end().len())
        });
        let mut wrapped = String::new();
        let _ = write!(
            wrapped,
            "; if {KEPT}({{{key_list}}}, \"{hash}\") then {targets} = {VALUES}()"
        );
        match init {
            Some(init) => {
                let _ = write!(wrapped, " else {targets} = {init} end");
            }
            None => wrapped.push_str(" end"),
        }
        let _ = write!(
            wrapped,
            "; {WATCH}({{{key_list}}}, \"{hash}\", function() return {targets} end)"
        );
        edits.push((declared_end..stat.span.end, wrapped));
    }
    if edits.is_empty() {
        return None;
    }
    let mut out = source.to_owned();
    for (range, text) in edits.into_iter().rev() {
        out.replace_range(range, &text);
    }
    Some(out)
}

/// A binding as written (`count: number`) without its type.
fn bare_name(binding: &str) -> &str {
    binding.split(':').next().unwrap_or(binding).trim()
}

/// Whether the line a statement ends on carries a `-- @reset` comment.
fn resets(source: &str, end: usize) -> bool {
    let rest = &source[end..];
    let line = rest.split('\n').next().unwrap_or(rest);
    line.find("--")
        .is_some_and(|at| line[at..].contains("@reset"))
}

/// Whether `expr` uses `...` outside any function of its own.
fn uses_vararg(expr: &lua::Expr) -> bool {
    match &expr.kind {
        ExprKind::Vararg => true,
        ExprKind::Field(inner, _) | ExprKind::Unary(_, inner) | ExprKind::Paren(inner) => {
            uses_vararg(inner)
        }
        ExprKind::Cast(inner, _) => uses_vararg(inner),
        ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => uses_vararg(a) || uses_vararg(b),
        ExprKind::Call(callee, args) => uses_vararg(callee) || args.iter().any(uses_vararg),
        ExprKind::Method(target, _, args) => uses_vararg(target) || args.iter().any(uses_vararg),
        ExprKind::Table(fields) => fields.iter().any(|field| match field {
            TableField::Named(_, value) | TableField::Item(value) => uses_vararg(value),
            TableField::Keyed(key, value) => uses_vararg(key) || uses_vararg(value),
        }),
        ExprKind::Interpolated(_, values) => values.iter().any(uses_vararg),
        ExprKind::IfElse(arms, otherwise) => {
            arms.iter().any(|(a, b)| uses_vararg(a) || uses_vararg(b)) || uses_vararg(otherwise)
        }
        ExprKind::Function(_)
        | ExprKind::Nil
        | ExprKind::True
        | ExprKind::False
        | ExprKind::Number(_)
        | ExprKind::Str(_)
        | ExprKind::Name(_) => false,
    }
}

/// FNV-1a over the initializer with its whitespace runs made one space, so
/// re-indenting a line doesn't count as an edit. The same in every run of
/// every build, unlike `std`'s hasher.
fn text_hash(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut space = false;
    for byte in text.trim().bytes() {
        if byte.is_ascii_whitespace() {
            space = true;
            continue;
        }
        if space {
            hash = (hash ^ u64::from(b' ')).wrapping_mul(0x0100_0000_01b3);
            space = false;
        }
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_is_wrapped_on_its_own_line() {
        let source = "--!strict\nlocal count: number = 0\nprint(count)\n";
        let wrapped = wrap(source).unwrap();
        let lines: Vec<&str> = wrapped.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "--!strict");
        assert!(
            lines[1].starts_with("local count: number; if __axcenity_kept({\"count\"}, \""),
            "{}",
            lines[1]
        );
        assert!(lines[1].contains("else count = 0 end;"), "{}", lines[1]);
        assert!(
            lines[1].ends_with("function() return count end)"),
            "{}",
            lines[1]
        );
        assert_eq!(lines[2], "print(count)");
    }

    #[test]
    fn a_multi_line_initializer_keeps_every_line_in_place() {
        let source = "local t = {\n  a = 1,\n}\nlocal x, y = 1, 2\nlocal z\n";
        let wrapped = wrap(source).unwrap();
        assert_eq!(wrapped.lines().count(), source.lines().count());
        assert!(wrapped.lines().nth(2).unwrap().starts_with("} end; "));
        assert!(wrapped.contains("then x, y = __axcenity_values() else x, y = 1, 2 end"));
        assert!(wrapped.contains("local z; if __axcenity_kept({\"z\"}"));
        assert!(wrapped.contains("then z = __axcenity_values() end;"));
    }

    #[test]
    fn locals_inside_functions_and_blocks_are_left_alone() {
        let source = "local function f()\n  local inner = 1\nend\ndo local d = 2 end\n";
        assert_eq!(wrap(source), None);
    }

    #[test]
    fn reset_self_reference_and_vararg_locals_always_run_their_initializer() {
        assert_eq!(wrap("local started = os.clock() -- @reset\n"), None);
        assert_eq!(wrap("local print = print\n"), None);
        assert_eq!(wrap("local args = { ... }\n"), None);
        // A function inside may use `...` of its own.
        assert!(wrap("local f = function(...) return ... end\n").is_some());
    }

    #[test]
    fn a_shadowed_local_has_its_own_key() {
        let wrapped = wrap("local x = 1\nlocal x = 2\n").unwrap();
        assert!(wrapped.contains("{\"x\"}"));
        assert!(wrapped.contains("{\"x#2\"}"));
    }

    #[test]
    fn the_hash_follows_the_initializer_not_its_layout() {
        let hash = |source: &str| {
            let wrapped = wrap(source).unwrap();
            let at = wrapped.find("}, \"").unwrap() + 4;
            wrapped[at..at + 16].to_owned()
        };
        assert_eq!(hash("local speed = 10\n"), hash("local speed  =   10\n"));
        assert_ne!(hash("local speed = 10\n"), hash("local speed = 20\n"));
    }

    #[test]
    fn code_the_reader_cant_read_is_not_wrapped() {
        assert_eq!(wrap("local x = (\n"), None);
    }
}
