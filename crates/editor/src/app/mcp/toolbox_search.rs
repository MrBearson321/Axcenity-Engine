//! `toolbox_search`: the Toolbox tab's search (`docs/editor/assets.md`)
//! of ambientCG, Poly Haven and Openverse, for a client to pick an asset
//! for `toolbox_get`. Sounds only under licences a game can use with at
//! most a credit, as the tab's default. Needs the network; blocks while
//! the sources answer.

use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};

use super::refs::quoted;
use super::{Host, LIMIT, after};
use crate::toolbox_panel::agent::{Hits, kind_named, search_assets};

pub const SPEC: ToolSpec = ToolSpec {
    name: "toolbox_search",
    title: "Search online assets",
    summary: "Search free textures, skies, models and sounds online (ambientCG, Poly Haven, Openverse); toolbox_get fetches one.",
    profile: Profile::Full,
    class: Class::Read,
    idempotent: true,
    params: &[
        Param::required("query", Kind::String, "words, e.g. \"rusty metal\""),
        Param::optional(
            "kind",
            Kind::Enum(&["texture", "sky", "model", "sound"]),
            "default texture",
        ),
        Param::optional("source", Kind::String, "ambientcg, polyhaven or openverse"),
        Param::optional("page", Kind::Integer, "from 1"),
        LIMIT,
    ],
};

/// Results shown when the client gives no limit.
const DEFAULT_LIMIT: usize = 10;

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let query = args.str("query").unwrap_or_default().trim().to_owned();
    if query.is_empty() {
        return Err(ToolError::bad_args("query is empty")
            .with_fix("give a few words, e.g. \"stone wall\" or \"forest birds\""));
    }
    let kind_name = args.str("kind").unwrap_or("texture");
    let kind = kind_named(kind_name).expect("the schema lists only these");
    let page = args.u64("page").unwrap_or(1).clamp(1, 1000) as u32;
    let source = args
        .str("source")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let http = host.session.toolbox_http();
    after(
        host,
        args,
        move || search_assets(&*http, kind, &query, source.as_deref(), page),
        answer,
    )
}

/// The results, a line each.
fn answer(_: &mut Host, args: &Args, found: Result<Hits, String>) -> Outcome {
    let found = found.map_err(|why| network_error("the search failed", why))?;
    let query = args.str("query").unwrap_or_default().trim();
    let kind_name = args.str("kind").unwrap_or("texture");
    let page = args.u64("page").unwrap_or(1).clamp(1, 1000);
    let limit = args.limit(DEFAULT_LIMIT, 24);
    let total = found.hits.len();
    let shown = total.min(limit);
    let mut out = format!(
        "{shown} of {total} {kind_name} result{} for {}{}",
        if total == 1 { "" } else { "s" },
        quoted(query),
        if found.more {
            format!(" (page {page}; more on page {})", page + 1)
        } else if page > 1 {
            format!(" (page {page})")
        } else {
            String::new()
        }
    );
    for hit in found.hits.iter().take(shown) {
        let mut line = format!("\n{} · {}", hit.key, hit.name);
        if !hit.category.is_empty() {
            line.push_str(&format!(" · {}", hit.category));
        }
        line.push_str(&format!(" · {}", hit.license));
        if hit.credit {
            line.push_str(" (credit needed)");
        }
        if hit.resolutions.is_empty() && hit.formats.is_empty() {
            line.push_str(" · sizes on get");
        } else {
            for list in [&hit.resolutions, &hit.formats] {
                if !list.is_empty() {
                    line.push_str(&format!(" · {}", list.join(" ")));
                }
            }
        }
        out.push_str(&line);
    }
    for failed in &found.failed {
        out.push_str(&format!("\nnot searched: {failed}"));
    }
    match found.hits.first() {
        Some(first) => out.push_str(&format!(
            "\nget one with toolbox_get {{\"key\":\"{}\"}}",
            first.key
        )),
        None => out.push_str("\ntry fewer or more general words"),
    }
    Ok(out)
}

/// A Toolbox failure: a wrong source or key is the client's to fix; the
/// rest (no connection, a source down) may pass.
pub(super) fn network_error(what: &str, why: String) -> ToolError {
    if why.contains("no source") || why.contains(" has no ") || why.contains("isn't an asset key") {
        return ToolError::new(Code::BadValue, format!("{what}: {why}"));
    }
    ToolError::new(Code::Busy, format!("{what}: {why}"))
        .with_fix("check this computer's internet connection, then try again")
}
