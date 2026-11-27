//! M042 layout-id-missing: a replaced Panorama layout that lacks an element id Valve's layout at the same path has on
//! the installed build.
//!
//! A mod ships a whole copy of a layout, so after a patch that adds or renames a panel the copy masks Valve's new file.
//! The game's code finds panels by id; when one it asks for is not there, the game stops at launch (`FATAL ERROR:
//! Unable to find child '...'`: seen on 6880 with a health-bar mod's `unit_status_overlay_v2` lacking `UnitHealthbar`,
//! the game never left its loading screen). Which ids the code asks for is in the client library itself (`Game::code_names`), so a missing id the
//! code names is an error and one it does not name (Valve's scripts may still look it up) is a warning.
//!
//! A layout from an older build can also use a panel type the game no longer registers. The game then cannot parse it
//! (`Unrecognized panel type`), and for a layout it requires that ends the game at launch: on 6880 a HUD mod's
//! `hud.xml` with eight removed panel types gave `FATAL ERROR: Unable to load layout file ... hud.xml` before any id was
//! looked up, and Steam then marked the install corrupt and verified it. The game's client library holds every panel
//! type it registers as a plain string (`Game::code_names`), so a type in neither it nor Valve's layout is said.
//!
//! A compiled layout holds its tree in a `LaCo` block (`m_AST`: PANEL nodes whose `id` attribute is a
//! PANEL_ATTRIBUTE); an older compile kept the XML text in DATA, read for ` id="..."`.

use resource::kv3::Value;
use std::collections::BTreeSet;

/// The element ids a compiled layout declares, or `None` when it holds no layout this reads.
pub fn ids(data: &[u8]) -> Option<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    if let Some(Ok(v)) = resource::kv3_block(data, b"LaCo") {
        v.walk(&mut |_, x| {
            if x.get("eType").and_then(Value::as_str) == Some("PANEL_ATTRIBUTE") && x.get("name").and_then(Value::as_str) == Some("id") {
                if let Some(n) = x.get("child").and_then(|c| c.get("name")).and_then(Value::as_str).filter(|n| !n.is_empty()) {
                    out.insert(n.to_string());
                }
            }
        });
        return Some(out);
    }
    let r = resource::parse(data).ok()?;
    let text = String::from_utf8_lossy(resource::block_bytes(data, r.block(b"DATA")?)).into_owned();
    if !text.contains("<root") && !text.contains(" id=\"") {
        return None;
    }
    for (i, _) in text.match_indices(" id=\"") {
        let rest = text.get(i + 5..).unwrap_or("");
        if let Some(n) = rest.split('"').next().filter(|n| !n.is_empty()) {
            out.insert(n.to_string());
        }
    }
    Some(out)
}

/// The ids Valve's layout has and ours lacks, split by whether the game's client code names them (`code`): the first
/// list is the one the game asks for. `None` when either side holds no layout this reads.
pub fn missing(ours: &[u8], valve: &[u8], code: Option<&std::collections::HashSet<String>>) -> Option<(Vec<String>, Vec<String>)> {
    let (o, v) = (ids(ours)?, ids(valve)?);
    let (mut named, mut rest) = (Vec::new(), Vec::new());
    for id in v.difference(&o) {
        if code.is_some_and(|c| c.contains(id)) {
            named.push(id.clone());
        } else {
            rest.push(id.clone());
        }
    }
    Some((named, rest))
}

/// The panel types a compiled layout's tree uses (its PANEL nodes' names), or `None` when it holds no `LaCo` tree.
pub fn panel_types(data: &[u8]) -> Option<BTreeSet<String>> {
    let v = resource::kv3_block(data, b"LaCo")?.ok()?;
    let mut out = BTreeSet::new();
    v.walk(&mut |_, x| {
        if x.get("eType").and_then(Value::as_str) == Some("PANEL") {
            if let Some(n) = x.get("name").and_then(Value::as_str).filter(|n| !n.is_empty()) {
                out.insert(n.to_string());
            }
        }
    });
    Some(out)
}

/// The panel types our layout uses that the game does not register: in neither Valve's layout at the same path nor
/// the client library's strings (`code`).
pub fn unknown_types(ours: &[u8], valve: &[u8], code: &std::collections::HashSet<String>) -> Vec<String> {
    let (Some(o), v) = (panel_types(ours), panel_types(valve).unwrap_or_default()) else { return Vec::new() };
    o.into_iter().filter(|t| !v.contains(t) && !code.contains(t)).collect()
}
