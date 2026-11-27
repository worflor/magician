//! `gameinfo.gi` read against the build's stock file, for `magician doctor`: the SearchPaths block line by line, and
//! the sections a config must not change.
//!
//! Every update or "verify files" puts the stock file back and drops the addons mount; other managers and old guides
//! paste whole SearchPaths blocks from earlier builds (City Never Sleeps renamed `Game_Language` to `Game_UILanguage`, so
//! a pasted block half-works), and "performance" configs edit sections the game checks before matchmaking (Engine2,
//! MaterialSystem2, NetworkSystem, Particles, RenderSystem, SceneSystem, WorldRenderer: `Unable to enter matchmaking
//! while any party member is running Tools-mode, or has unsupported changes`, on one community page, UNVERIFIED here).
//! The stock file is the backup `install` keeps of each build's file before its first edit.

/// A KeyValues node: a key with a value or a block of nodes.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Leaf(String, String),
    Block(String, Vec<Node>),
}

impl Node {
    pub fn key(&self) -> &str {
        match self {
            Node::Leaf(k, _) | Node::Block(k, _) => k,
        }
    }
    /// The node as one line of text with no comments or layout, keys as written: what two files share or not.
    pub fn canonical(&self) -> String {
        match self {
            Node::Leaf(k, v) => format!("{k} {v:?}"),
            Node::Block(k, c) => format!("{k} {{ {} }}", c.iter().map(Node::canonical).collect::<Vec<_>>().join(" ")),
        }
    }
}

/// The sections whose change the community reports keeps a player out of matchmaking.
pub const RESTRICTED: &[&str] = &["Engine2", "MaterialSystem2", "NetworkSystem", "Particles", "RenderSystem", "SceneSystem", "WorldRenderer"];

/// The nodes of a KeyValues text (`//` comments, quoted or bare tokens, `{` `}` blocks; a conditional `[$X]` after a
/// value is kept with it). A text that does not close its blocks is read as far as it goes.
pub fn parse(text: &str) -> Vec<Node> {
    let mut toks: Vec<String> = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' => {
                let start = i + 1;
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += 1;
                }
                toks.push(format!("\"{}", text.get(start..i).unwrap_or("")));
                i += 1;
            }
            b'{' | b'}' => {
                toks.push((b[i] as char).to_string());
                i += 1;
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                let start = i;
                // a bare token ends at white space, a brace, a quote or a comment
                let ends = |i: usize| i >= b.len() || b[i].is_ascii_whitespace() || matches!(b[i], b'{' | b'}' | b'"') || (b[i] == b'/' && b.get(i + 1) == Some(&b'/'));
                while !ends(i) {
                    i += 1;
                }
                toks.push(format!("\"{}", text.get(start..i).unwrap_or("")));
            }
        }
    }
    let mut at = 0;
    nodes(&toks, &mut at)
}

fn nodes(t: &[String], at: &mut usize) -> Vec<Node> {
    let mut out = Vec::new();
    while *at < t.len() {
        let tok = &t[*at];
        if tok == "}" {
            *at += 1;
            return out;
        }
        if tok == "{" {
            // a block with no key: read through it
            *at += 1;
            out.extend(nodes(t, at));
            continue;
        }
        let key = tok.trim_start_matches('"').to_string();
        *at += 1;
        match t.get(*at).map(String::as_str) {
            Some("{") => {
                *at += 1;
                out.push(Node::Block(key, nodes(t, at)));
            }
            Some(v) if v != "}" => {
                *at += 1;
                out.push(Node::Leaf(key, v.trim_start_matches('"').to_string()));
            }
            _ => out.push(Node::Leaf(key, String::new())),
        }
    }
    out
}

/// The sections of the file's root block (`GameInfo { ... }`), by key.
fn sections(text: &str) -> Vec<Node> {
    parse(text).into_iter().find_map(|n| match n {
        Node::Block(k, c) if k.eq_ignore_ascii_case("GameInfo") => Some(c),
        _ => None,
    }).unwrap_or_default()
}

/// The `FileSystem.SearchPaths` entries of a file, as (key, value).
pub fn search_paths(text: &str) -> Vec<(String, String)> {
    let find = |list: &[Node], key: &str| list.iter().find_map(|n| match n {
        Node::Block(k, c) if k.eq_ignore_ascii_case(key) => Some(c.clone()),
        _ => None,
    });
    find(&sections(text), "FileSystem").and_then(|fs| find(&fs, "SearchPaths")).unwrap_or_default().into_iter().filter_map(|n| match n {
        Node::Leaf(k, v) => Some((k, v)),
        _ => None,
    }).collect()
}

/// What a `gameinfo.gi` changes against the stock file of its build.
#[derive(Debug, Default, PartialEq)]
pub struct Diff {
    /// Stock SearchPaths entries the file lacks (`key value`).
    pub missing: Vec<String>,
    /// Entries the file adds that are not Magician's addons mount (another manager's, or a pasted block's).
    pub added: Vec<String>,
    /// Restricted sections that differ from the stock file's, or that the stock file has not.
    pub restricted: Vec<String>,
}

/// The entries `install` writes: the addons mount, and the `Mod` and `Write` roots kept on `citadel` when it is first.
fn magicians(k: &str, v: &str) -> bool {
    (k.eq_ignore_ascii_case("Game") && v.eq_ignore_ascii_case("citadel/addons")) || ((k.eq_ignore_ascii_case("Mod") || k.eq_ignore_ascii_case("Write")) && v.eq_ignore_ascii_case("citadel"))
}

/// `current` against `stock`.
pub fn diff(current: &str, stock: &str) -> Diff {
    let line = |(k, v): &(String, String)| format!("{k} {v}");
    let same = |a: &(String, String), b: &(String, String)| a.0.eq_ignore_ascii_case(&b.0) && a.1.eq_ignore_ascii_case(&b.1);
    let (cur, st) = (search_paths(current), search_paths(stock));
    let missing = st.iter().filter(|s| !cur.iter().any(|c| same(c, s))).map(line).collect();
    let added = cur.iter().filter(|c| !st.iter().any(|s| same(c, s)) && !magicians(&c.0, &c.1)).map(line).collect();
    let (cs, ss) = (sections(current), sections(stock));
    let restricted = RESTRICTED
        .iter()
        .filter(|r| {
            let of = |list: &[Node]| list.iter().filter(|n| n.key().eq_ignore_ascii_case(r)).map(Node::canonical).collect::<Vec<_>>();
            of(&cs) != of(&ss)
        })
        .map(|r| r.to_string())
        .collect();
    Diff { missing, added, restricted }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STOCK: &str = "\"GameInfo\"\n{\n\tFileSystem\n\t{\n\t\tSearchPaths\n\t\t{\n\t\t\t// localized images\n\t\t\tGame_UILanguage\t\tcitadel_*LANGUAGE*\n\t\t\tGame_LowViolence\tcitadel_lv\n\t\t\tGame\t\t\t\tcitadel\n\t\t\tGame\t\t\t\tcore\n\t\t}\n\t}\n\tRenderSystem\n\t{\n\t\tVulkanUseSecondaryCommandBuffers 1\n\t}\n\tEngine2\n\t{\n\t\tHasModAppSystems 1 // a comment\n\t}\n}\n";

    #[test]
    fn the_stock_file_against_itself_and_with_magician_s_mount() {
        assert_eq!(diff(STOCK, STOCK), Diff::default());
        let mounted = STOCK.replace("\t\t\tGame\t\t\t\tcitadel\n", "\t\t\tMod\t\t\t\tcitadel\n\t\t\tWrite\t\t\t\tcitadel\n\t\t\tGame\t\t\t\tcitadel/addons\n\t\t\tGame\t\t\t\tcitadel\n");
        assert_eq!(diff(&mounted, STOCK), Diff::default(), "Magician's own mount is no change");
        // comments and layout are not changes
        assert_eq!(diff(&STOCK.replace("HasModAppSystems 1 // a comment", "\"HasModAppSystems\"   \"1\""), STOCK), Diff::default());
    }

    #[test]
    fn a_block_pasted_from_an_older_build_and_a_performance_config() {
        // City Never Sleeps renamed Game_Language to Game_UILanguage: an older guide's block lacks the new line
        let pasted = STOCK.replace("Game_UILanguage\t\tcitadel_*LANGUAGE*", "Game_Language\t\tcitadel_*LANGUAGE*");
        let d = diff(&pasted, STOCK);
        assert_eq!(d.missing, vec!["Game_UILanguage citadel_*LANGUAGE*"]);
        assert_eq!(d.added, vec!["Game_Language citadel_*LANGUAGE*"]);
        assert!(d.restricted.is_empty());
        // a config that changes a restricted section, or adds one the stock file has not
        let tuned = STOCK.replace("VulkanUseSecondaryCommandBuffers 1", "VulkanUseSecondaryCommandBuffers 0").replace("\tEngine2\n", "\tParticles\n\t{\n\t\tMaxParticles 64\n\t}\n\tEngine2\n");
        assert_eq!(diff(&tuned, STOCK).restricted, vec!["Particles", "RenderSystem"]);
    }

    #[test]
    fn search_paths_are_read_from_the_file_system_block() {
        assert_eq!(search_paths(STOCK).len(), 4);
        assert_eq!(search_paths(STOCK)[0], ("Game_UILanguage".to_string(), "citadel_*LANGUAGE*".to_string()));
        assert!(search_paths("not keyvalues {").is_empty());
    }
}
