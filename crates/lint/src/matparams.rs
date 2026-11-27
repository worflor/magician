//! The material parameters and features Valve's own materials set, per shader and by the kind each is stored as, on
//! one build (`material_params.tsv` in the build's reference folder). The CSDK's compiler keeps every name a `.vmat`
//! gives it that the shader does not know, and types a value by its text (`"1"` is an integer, `"1.0"` a float), so
//! a compiled material can neither say which of its names the shader reads nor that a float came out an integer
//! (docs/reference/engine.md, Materials). What Valve's shipped materials set is the evidence: a name no Valve
//! material of that shader sets is one the shader most likely ignores (M150), and a name Valve stores only as a
//! float that a mod stores as an integer is one the shader does not get (M151).

use std::collections::BTreeMap;

/// shader (lowercase) -> `kind:name` -> how many of Valve's materials of that shader set it; `""` counts the
/// shader's materials.
pub type MaterialParams = BTreeMap<String, BTreeMap<String, u32>>;

/// The header line a cache must start with to be read: a file from an older format is collected again.
pub fn header(build: &str) -> String {
    format!("# magician material params build={build} rev=2")
}

/// Every name a compiled material sets, with the kind it is stored as: `feature`, `int`, `float`, `vector`,
/// `texture`, `dynamic`.
pub fn names(m: &resource::material::Material) -> Vec<(&'static str, String)> {
    let mut v: Vec<(&'static str, String)> = m
        .ints
        .iter()
        .map(|(n, _)| (if n.starts_with("F_") { "feature" } else { "int" }, n.clone()))
        .chain(m.floats.iter().map(|(n, _)| ("float", n.clone())))
        .chain(m.vectors.iter().map(|(n, _)| ("vector", n.clone())))
        .chain(m.textures.iter().map(|(n, _)| ("texture", n.clone())))
        .chain(m.dynamic.iter().map(|(n, _)| ("dynamic", n.clone())))
        .collect();
    v.sort();
    v.dedup();
    v
}

pub fn collect(m: &resource::material::Material, into: &mut MaterialParams) {
    let s = into.entry(m.shader.to_ascii_lowercase()).or_default();
    *s.entry(String::new()).or_insert(0) += 1;
    for (k, n) in names(m) {
        *s.entry(format!("{k}:{n}")).or_insert(0) += 1;
    }
}

pub fn to_tsv(p: &MaterialParams, build: &str) -> String {
    let mut out = format!("{}\n# shader\tkind:name\tmaterials (\"\" = the shader's material count)\n", header(build));
    for (shader, names) in p {
        for (n, c) in names {
            out += &format!("{shader}\t{n}\t{c}\n");
        }
    }
    out
}

pub fn parse_tsv(text: &str) -> MaterialParams {
    let mut p = MaterialParams::new();
    for l in text.lines().filter(|l| !l.starts_with('#')) {
        let mut f = l.split('\t');
        let (Some(s), Some(n), Some(c)) = (f.next(), f.next(), f.next()) else { continue };
        let Ok(c) = c.trim().parse::<u32>() else { continue };
        p.entry(s.to_string()).or_default().insert(n.to_string(), c);
    }
    p
}

/// The kinds Valve's materials of a shader store a name as (lowercase name match).
fn kinds_of<'a>(known: &'a BTreeMap<String, u32>, name: &str) -> Vec<&'a str> {
    known.keys().filter_map(|k| k.split_once(':')).filter(|(_, n)| n.eq_ignore_ascii_case(name)).map(|(k, _)| k).collect()
}

/// The names of `m` that no Valve material of its shader sets, or None when Valve ships no material of that shader
/// (nothing to judge by).
pub fn unread(m: &resource::material::Material, p: &MaterialParams) -> Option<Vec<String>> {
    let known = p.get(&m.shader.to_ascii_lowercase())?;
    let lower: std::collections::HashSet<String> = known.keys().filter_map(|k| k.split_once(':')).map(|(_, n)| n.to_ascii_lowercase()).collect();
    let mut out: Vec<String> = names(m).into_iter().map(|(_, n)| n).filter(|n| !lower.contains(&n.to_ascii_lowercase())).collect();
    out.dedup();
    Some(out)
}

/// The names `m` stores as an integer that every Valve material of its shader setting them stores as a float or a
/// vector (and the other way round): `(name, ours, Valve's)`. A name Valve stores both ways is Valve's design.
pub fn mistyped(m: &resource::material::Material, p: &MaterialParams) -> Option<Vec<(String, &'static str, String)>> {
    let known = p.get(&m.shader.to_ascii_lowercase())?;
    let mut out = Vec::new();
    for (kind, n) in names(m) {
        if !matches!(kind, "int" | "float" | "vector") {
            continue;
        }
        // a dynamic expression of the same name is beside the point: what counts is how the constant is stored
        let mut valve = kinds_of(known, &n);
        valve.retain(|k| matches!(*k, "int" | "float" | "vector"));
        valve.sort();
        valve.dedup();
        if !valve.is_empty() && !valve.contains(&kind) {
            out.push((n, kind, valve.join(" or ")));
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use resource::material::Material;

    fn mat(shader: &str, names: &[&str]) -> Material {
        Material { shader: shader.into(), floats: names.iter().map(|n| (n.to_string(), 1.0)).collect(), compiled: true, ..Material::default() }
    }

    #[test]
    fn names_valve_does_not_set_are_unread() {
        let mut p = MaterialParams::new();
        collect(&mat("pbr.vfx", &["g_flA", "g_flB"]), &mut p);
        collect(&mat("pbr.vfx", &["g_flA"]), &mut p);
        assert_eq!(p["pbr.vfx"]["float:g_flA"], 2);
        assert_eq!(p["pbr.vfx"][""], 2);
        let back = parse_tsv(&to_tsv(&p, "6875"));
        assert_eq!(back, p);
        assert_eq!(unread(&mat("PBR.vfx", &["g_fla", "g_flRimLightScale1"]), &back), Some(vec!["g_flRimLightScale1".to_string()]));
        assert_eq!(unread(&mat("other.vfx", &["g_flA"]), &back), None);
    }

    #[test]
    fn a_float_stored_as_an_integer_is_mistyped() {
        let mut p = MaterialParams::new();
        collect(&mat("pbr.vfx", &["g_flSelfIllumScale1"]), &mut p);
        let mut ours = Material { shader: "pbr.vfx".into(), ints: vec![("g_flSelfIllumScale1".into(), 1)], compiled: true, ..Material::default() };
        assert_eq!(mistyped(&ours, &p), Some(vec![("g_flSelfIllumScale1".to_string(), "int", "float".to_string())]));
        // a name Valve stores both ways is its design
        collect(&ours, &mut p);
        assert_eq!(mistyped(&ours, &p), Some(vec![]));
        ours.ints.clear();
        assert_eq!(mistyped(&ours, &p), Some(vec![]));
    }
}
