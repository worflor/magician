//! A material as data: a `.vmat` source (KeyValues 1, `"Layer0" { ... }`, or KV3 text) or a compiled `.vmat_c`
//! (its `DATA` block, and from its `RED2` edit info the source images it was compiled from), read into one shape so
//! two can be compared, a setup carried to another project, and a compile held to its source.
//!
//! Names on the two sides differ. A source names its inputs `Texture<Role><layer>` (`TextureColor1`) and the compiler
//! packs them into the shader's texture slots (`g_tColor`), several inputs to one slot: `ROUTES` is that packing for
//! Deadlock's `pbr.vfx`, as measured on CSDK 13 against probe materials (docs/reference/engine.md, Materials).
//! Parameters (`g_fl*`, `g_v*`, `g_n*`, `g_b*`) and features (`F_*`) keep their names. The compiler keeps every
//! parameter and feature it is given, known to the shader or not, so a compiled material does not say which ones
//! the shader reads.

use crate::kv3::Value;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Material {
    /// The material's own path (`materials/x/y.vmat`), when the file says it.
    pub name: String,
    pub shader: String,
    /// Features (`F_*`) and integer parameters, by name.
    pub ints: Vec<(String, i64)>,
    pub floats: Vec<(String, f64)>,
    pub vectors: Vec<(String, [f64; 4])>,
    /// Compiled: slot (`g_tColor`) to texture (`...vtex`). Source: input (`TextureColor1`) to image or `.vtex`.
    pub textures: Vec<(String, String)>,
    /// Source inputs given as a colour instead of an image (`"TextureRoughness1" "[1 1 1 0]"`).
    pub constants: Vec<(String, [f64; 4])>,
    /// Dynamic parameters: the expression (source) or its compiled byte count (compiled).
    pub dynamic: Vec<(String, String)>,
    /// Compiled only: the source images the compile read (`RED2` image arguments), Valve's defaults left out.
    pub inputs: Vec<String>,
    /// Compiled only: the render attributes a dynamic expression reads (`$ent_abs_velocity`).
    pub render_attributes: Vec<String>,
    /// Source keys that are none of the above.
    pub other: Vec<(String, String)>,
    pub compiled: bool,
}

/// One input of Deadlock's `pbr.vfx` and where the compiler puts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    /// The source input without its layer digit (`TextureColor`).
    pub input: &'static str,
    /// The slot it lands in, and the channels of that slot it fills (`rgb`, `a`, `r`...).
    pub slot: &'static str,
    pub channels: &'static str,
    /// The feature that sends it elsewhere, when one does: under it the input is in `alt_slot`.`alt_channels`.
    pub unless: &'static str,
    pub alt_slot: &'static str,
    pub alt_channels: &'static str,
    /// What the channel means to the shader.
    pub meaning: &'static str,
}

const fn r(input: &'static str, slot: &'static str, channels: &'static str, meaning: &'static str) -> Route {
    Route { input, slot, channels, unless: "", alt_slot: "", alt_channels: "", meaning }
}

/// Deadlock `pbr.vfx`'s packing, measured on CSDK 13 (probe materials with one pattern per input, every compiled
/// channel matched against every input channel; docs/reference/engine.md, Materials).
pub const ROUTES: &[Route] = &[
    r("TextureColor", "g_tColor", "rgb", "albedo (sRGB)"),
    Route { input: "TextureMetalness", slot: "g_tColor", channels: "a", unless: "F_TRANSLUCENT", alt_slot: "g_tMetalness", alt_channels: "r", meaning: "metalness" },
    r("TextureTranslucency", "g_tColor", "a", "coverage (alpha)"),
    r("TextureNormal", "g_tNormalRoughness", "rg", "normal, hemi-octahedral"),
    r("TextureRoughness", "g_tNormalRoughness", "b", "roughness (mips filtered by the normal's variance)"),
    r("TextureSheenColor", "g_tSheen", "rgb", "sheen colour"),
    r("TextureSheenRoughness", "g_tSheen", "a", "sheen roughness"),
    Route { input: "TextureTintMask", slot: "g_tTintMaskRimLightMask", channels: "r", unless: "F_TRANSLUCENT", alt_slot: "g_tTintMask", alt_channels: "r", meaning: "colour-tint mask" },
    r("TextureRimLightMask", "g_tTintMaskRimLightMask", "g", "rim-light mask (the rim's strength is the scene's)"),
    r("TextureSelfIllumMask", "g_tSelfIllumMask", "r", "self-illumination mask"),
    r("TextureAmbientOcclusion", "g_tAmbientOcclusion", "r", "ambient occlusion"),
    r("TextureNprOutlineMask", "g_tNprOutlineMask", "r", "outline mask"),
    r("TextureNprTramsissiveColor", "g_tNprTransmissiveColor", "rgb", "NPR transmissive colour (Valve's spelling)"),
];

impl Route {
    /// The slot and channels under the material's features.
    pub fn place(&self, m: &Material) -> (&'static str, &'static str) {
        if !self.unless.is_empty() && m.int(self.unless).unwrap_or(0) != 0 {
            (self.alt_slot, self.alt_channels)
        } else {
            (self.slot, self.channels)
        }
    }
}

/// A source input's name without its layer digits: `TextureColor1` is `TextureColor`.
pub fn input_role(key: &str) -> &str {
    key.trim_end_matches(|c: char| c.is_ascii_digit())
}

pub fn route_of(key: &str) -> Option<&'static Route> {
    let role = input_role(key);
    ROUTES.iter().find(|r| r.input.eq_ignore_ascii_case(role))
}

impl Material {
    pub fn int(&self, k: &str) -> Option<i64> {
        self.ints.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| *v)
    }
    pub fn float(&self, k: &str) -> Option<f64> {
        self.floats.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| *v)
    }
    pub fn vector(&self, k: &str) -> Option<[f64; 4]> {
        self.vectors.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| *v)
    }
    pub fn texture(&self, k: &str) -> Option<&str> {
        self.textures.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.as_str())
    }
    /// The features (`F_*`) set to anything but 0.
    pub fn features(&self) -> Vec<(&str, i64)> {
        self.ints.iter().filter(|(n, v)| n.starts_with("F_") && *v != 0).map(|(n, v)| (n.as_str(), *v)).collect()
    }
    fn sort(&mut self) {
        self.ints.sort_by(|a, b| a.0.cmp(&b.0));
        self.floats.sort_by(|a, b| a.0.cmp(&b.0));
        self.vectors.sort_by(|a, b| a.0.cmp(&b.0));
        self.textures.sort_by(|a, b| a.0.cmp(&b.0));
        self.constants.sort_by(|a, b| a.0.cmp(&b.0));
        self.dynamic.sort_by(|a, b| a.0.cmp(&b.0));
        self.other.sort_by(|a, b| a.0.cmp(&b.0));
        self.inputs.sort();
        self.inputs.dedup();
    }
}

fn vec4(v: &[f64]) -> [f64; 4] {
    let g = |i: usize| v.get(i).copied().unwrap_or(0.0);
    [g(0), g(1), g(2), g(3)]
}

/// A compiled `.vmat_c`.
pub fn from_compiled(data: &[u8]) -> Result<Material, String> {
    let d = crate::kv3_block(data, b"DATA").ok_or("no DATA block: not a compiled material")??;
    if d.get("m_shaderName").is_none() {
        return Err("DATA holds no m_shaderName: not a compiled material".into());
    }
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let mut m = Material { name: s(&d, "m_materialName"), shader: s(&d, "m_shaderName"), compiled: true, ..Material::default() };
    for p in d.get("m_intParams").map(Value::as_array).unwrap_or(&[]) {
        m.ints.push((s(p, "m_name"), p.get("m_nValue").and_then(Value::as_f64).unwrap_or(0.0) as i64));
    }
    for p in d.get("m_floatParams").map(Value::as_array).unwrap_or(&[]) {
        m.floats.push((s(p, "m_name"), p.get("m_flValue").and_then(Value::as_f64).unwrap_or(0.0)));
    }
    for p in d.get("m_vectorParams").map(Value::as_array).unwrap_or(&[]) {
        m.vectors.push((s(p, "m_name"), vec4(&p.get("m_value").map(Value::numbers).unwrap_or_default())));
    }
    for p in d.get("m_textureParams").map(Value::as_array).unwrap_or(&[]) {
        m.textures.push((s(p, "m_name"), s(p, "m_pValue")));
    }
    for p in d.get("m_dynamicParams").map(Value::as_array).unwrap_or(&[]) {
        let n = match p.get("m_value") {
            Some(Value::Blob(b)) => b.len(),
            _ => 0,
        };
        m.dynamic.push((s(p, "m_name"), format!("compiled, {n} bytes")));
    }
    for p in d.get("m_dynamicTextureParams").map(Value::as_array).unwrap_or(&[]) {
        m.dynamic.push((s(p, "m_name"), "compiled texture expression".into()));
    }
    m.render_attributes = d.get("m_renderAttributesUsed").map(|v| v.strings().iter().map(|x| x.to_string()).collect()).unwrap_or_default();
    if let Some(Ok(red)) = crate::kv3_block(data, b"RED2") {
        for a in red.get("m_ArgumentDependencies").map(Value::as_array).unwrap_or(&[]) {
            let name = s(a, "m_ParameterName");
            if s(a, "m_ParameterType") == "ImageArg" && !name.starts_with("materials/default/") {
                m.inputs.push(name);
            }
        }
    }
    m.sort();
    Ok(m)
}

/// One token of KeyValues 1 text.
#[derive(Debug, PartialEq)]
enum Tok {
    Str(String),
    Open,
    Close,
}

fn kv1_tokens(src: &str) -> Result<Vec<Tok>, String> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b.get(i).copied().unwrap_or(0);
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b.get(i) != Some(&b'\n') {
                i += 1;
            }
        } else if c == b'{' {
            out.push(Tok::Open);
            i += 1;
        } else if c == b'}' {
            out.push(Tok::Close);
            i += 1;
        } else if c == b'"' {
            let mut v = Vec::new();
            i += 1;
            loop {
                match b.get(i) {
                    None => return Err("kv1: unterminated string".into()),
                    Some(b'"') => break,
                    Some(b'\\') => {
                        match b.get(i + 1) {
                            Some(b'n') => v.push(b'\n'),
                            Some(b't') => v.push(b'\t'),
                            Some(&x) => v.push(x),
                            None => return Err("kv1: unterminated string".into()),
                        }
                        i += 1;
                    }
                    Some(&x) => v.push(x),
                }
                i += 1;
            }
            i += 1;
            out.push(Tok::Str(String::from_utf8_lossy(&v).into_owned()));
        } else {
            let start = i;
            while i < b.len() && !b.get(i).is_some_and(|x| x.is_ascii_whitespace() || *x == b'{' || *x == b'}' || *x == b'"') {
                i += 1;
            }
            out.push(Tok::Str(String::from_utf8_lossy(b.get(start..i).unwrap_or(&[])).into_owned()));
        }
    }
    Ok(out)
}

/// KeyValues 1 text as nested objects (a repeated key keeps every occurrence, in order).
pub fn parse_kv1(src: &str) -> Result<Value, String> {
    let toks = kv1_tokens(src.trim_start_matches('\u{feff}'))?;
    let mut pos = 0;
    fn object(t: &[Tok], pos: &mut usize, depth: usize) -> Result<Value, String> {
        if depth > 64 {
            return Err("kv1: nested too deep".into());
        }
        let mut m = Vec::new();
        loop {
            match t.get(*pos) {
                None => return Ok(Value::Object(m)),
                Some(Tok::Close) => {
                    *pos += 1;
                    return Ok(Value::Object(m));
                }
                Some(Tok::Open) => return Err("kv1: a block with no key".into()),
                Some(Tok::Str(k)) => {
                    *pos += 1;
                    match t.get(*pos) {
                        Some(Tok::Str(v)) => {
                            *pos += 1;
                            m.push((k.clone(), Value::Str(v.clone())));
                        }
                        Some(Tok::Open) => {
                            *pos += 1;
                            m.push((k.clone(), object(t, pos, depth + 1)?));
                        }
                        _ => return Err(format!("kv1: key {k:?} has no value")),
                    }
                }
            }
        }
    }
    object(&toks, &mut pos, 0)
}

/// `[1 0.5 0 1]` or `[1, 0.5, 0, 1]` as numbers, None when it is not a bracketed list.
pub fn parse_vector(s: &str) -> Option<[f64; 4]> {
    let t = s.trim();
    let inner = t.strip_prefix('[')?.strip_suffix(']')?;
    let n: Vec<f64> = inner.split(|c: char| c.is_ascii_whitespace() || c == ',').filter(|x| !x.is_empty()).map(|x| x.parse::<f64>()).collect::<Result<_, _>>().ok()?;
    if n.is_empty() || n.len() > 4 {
        return None;
    }
    Some(vec4(&n))
}

fn scalar_text(v: &Value) -> Option<String> {
    Some(match v {
        Value::Str(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::UInt(u) => u.to_string(),
        Value::Double(d) => d.to_string(),
        Value::Bool(b) => (*b as u8).to_string(),
        Value::Array(a) => format!("[{}]", a.iter().filter_map(Value::as_f64).map(|x| x.to_string()).collect::<Vec<_>>().join(" ")),
        _ => return None,
    })
}

/// A `.vmat` source: KeyValues 1 (`"Layer0" { "shader" "pbr.vfx" ... }`) or KV3 text (`{ Layer0 = { ... } }`).
pub fn from_source(text: &str) -> Result<Material, String> {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    let root = if t.starts_with("<!--") { crate::kv3::parse_text(t)? } else { parse_kv1(t)? };
    // the first layer block: `Layer0` at the root, or the root itself when it already holds the shader
    let layer = root
        .members()
        .iter()
        .find(|(k, v)| k.to_ascii_lowercase().starts_with("layer") && matches!(v, Value::Object(_)))
        .map(|(_, v)| v)
        .or_else(|| root.get("shader").map(|_| &root))
        .ok_or("no Layer0 block and no shader: not a material source")?;
    let mut m = Material::default();
    for (k, v) in layer.members() {
        if k.eq_ignore_ascii_case("DynamicParams") || k.eq_ignore_ascii_case("DynamicTextureParams") {
            for (dk, dv) in v.members() {
                m.dynamic.push((dk.clone(), scalar_text(dv).unwrap_or_default()));
            }
            continue;
        }
        let Some(val) = scalar_text(v) else {
            m.other.push((k.clone(), "{...}".into()));
            continue;
        };
        let lk = k.to_ascii_lowercase();
        if lk == "shader" {
            m.shader = val;
        } else if k.starts_with("F_") {
            m.ints.push((k.clone(), val.trim().parse::<f64>().unwrap_or(0.0) as i64));
        } else if lk.starts_with("texture") {
            match parse_vector(&val) {
                Some(c) => m.constants.push((k.clone(), c)),
                None => m.textures.push((k.clone(), val.replace('\\', "/"))),
            }
        } else if lk.starts_with("g_v") {
            match parse_vector(&val) {
                Some(c) => m.vectors.push((k.clone(), c)),
                None => m.other.push((k.clone(), val)),
            }
        } else if lk.starts_with("g_fl") || lk.starts_with("g_f") {
            match val.trim().parse::<f64>() {
                Ok(x) => m.floats.push((k.clone(), x)),
                Err(_) => m.other.push((k.clone(), val)),
            }
        } else if lk.starts_with("g_n") || lk.starts_with("g_b") || lk.starts_with("g_i") {
            match val.trim().parse::<f64>() {
                Ok(x) => m.ints.push((k.clone(), x as i64)),
                Err(_) => m.other.push((k.clone(), val)),
            }
        } else {
            m.other.push((k.clone(), val));
        }
    }
    if m.shader.is_empty() {
        return Err("the material names no shader".into());
    }
    m.sort();
    Ok(m)
}

/// The compiled slot a source input should land in, under the source's features, and its channels.
/// None for an input ROUTES does not know, and for every input of a shader other than `pbr.vfx` (the routes are its).
pub fn expected_slot(src: &Material, input_key: &str) -> Option<(&'static str, &'static str, &'static Route)> {
    if !src.shader.eq_ignore_ascii_case("pbr.vfx") {
        return None;
    }
    let r = route_of(input_key)?;
    let (s, c) = r.place(src);
    Some((s, c, r))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAID: &str = "// a comment\n\"Layer0\"\n{\n\t\"shader\"\t\"pbr.vfx\"\n\t\"F_SHEEN\"\t\"1\"\n\t\"TextureColor1\"\t\"materials/x/a_color.png\"\n\t\"TextureMetalness1\"\t\"materials/x/a_metal.png\"\n\t\"TextureNprTramsissiveColor1\"\t\"[0.000000 0.5 0.000000 0.000000]\"\n\t\"g_flSelfIllumScale1\"\t\"1.000\"\n\t\"g_vSheenColorTint1\"\t\"[1 1 1 0]\"\n\t\"DynamicParams\"\n\t{\n\t\t\"g_vSheenColorTint1\"\t\"return float3(1,1,1);\"\n\t}\n}\n";

    #[test]
    fn a_kv1_source_reads_into_its_parts() {
        let m = from_source(MAID).unwrap();
        assert_eq!(m.shader, "pbr.vfx");
        assert_eq!(m.features(), [("F_SHEEN", 1)]);
        assert_eq!(m.textures.len(), 2);
        assert_eq!(m.constants, [("TextureNprTramsissiveColor1".to_string(), [0.0, 0.5, 0.0, 0.0])]);
        assert_eq!(m.float("g_flselfillumscale1"), Some(1.0));
        assert_eq!(m.vector("g_vSheenColorTint1"), Some([1.0, 1.0, 1.0, 0.0]));
        assert_eq!(m.dynamic, [("g_vSheenColorTint1".to_string(), "return float3(1,1,1);".to_string())]);
        assert!(!m.compiled);
    }

    #[test]
    fn a_kv3_source_reads_the_same() {
        let t = "<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} format:generic:version{7412167c-06e9-4698-aff2-e63eb59037e7} -->\n{\n\tLayer0 = \n\t{\n\t\tshader = \"pbr.vfx\"\n\t\tF_SHEEN = 1\n\t\tTextureColor1 = \"materials/x/a_color.png\"\n\t\tg_flSelfIllumScale1 = 1.0\n\t}\n}\n";
        let m = from_source(t).unwrap();
        assert_eq!((m.shader.as_str(), m.features().len(), m.textures.len(), m.float("g_flSelfIllumScale1")), ("pbr.vfx", 1, 1, Some(1.0)));
    }

    #[test]
    fn routes_follow_the_features() {
        let mut m = from_source(MAID).unwrap();
        assert_eq!(expected_slot(&m, "TextureMetalness1").map(|x| (x.0, x.1)), Some(("g_tColor", "a")));
        m.ints.push(("F_TRANSLUCENT".into(), 1));
        assert_eq!(expected_slot(&m, "TextureMetalness1").map(|x| (x.0, x.1)), Some(("g_tMetalness", "r")));
        assert_eq!(expected_slot(&m, "TextureTranslucency1").map(|x| x.0), Some("g_tColor"));
        assert!(expected_slot(&m, "TextureMagic1").is_none());
        assert_eq!(input_role("TextureColor12"), "TextureColor");
    }

    #[test]
    fn a_compiled_material_reads_its_slots_and_inputs() {
        let data = Value::Object(vec![
            ("m_materialName".into(), Value::Str("materials/x/a.vmat".into())),
            ("m_shaderName".into(), Value::Str("pbr.vfx".into())),
            ("m_intParams".into(), Value::Array(vec![Value::Object(vec![("m_name".into(), Value::Str("F_SHEEN".into())), ("m_nValue".into(), Value::Int(1))])])),
            ("m_vectorParams".into(), Value::Array(vec![Value::Object(vec![("m_name".into(), Value::Str("g_vTint".into())), ("m_value".into(), Value::Array(vec![Value::Double(1.0), Value::Double(0.5), Value::Double(0.0), Value::Double(0.0)]))])])),
            ("m_textureParams".into(), Value::Array(vec![Value::Object(vec![("m_name".into(), Value::Str("g_tColor".into())), ("m_pValue".into(), Value::Str("materials/x/a_color_png_1.vtex".into()))])])),
        ]);
        let red = Value::Object(vec![(
            "m_ArgumentDependencies".into(),
            Value::Array(vec![
                Value::Object(vec![("m_ParameterName".into(), Value::Str("materials/x/a_color.png".into())), ("m_ParameterType".into(), Value::Str("ImageArg".into()))]),
                Value::Object(vec![("m_ParameterName".into(), Value::Str("materials/default/default_color.tga".into())), ("m_ParameterType".into(), Value::Str("ImageArg".into()))]),
            ]),
        )]);
        let b = crate::synth_with(&[], &[(*b"RED2", crate::kv3::encode(&red)), (*b"DATA", crate::kv3::encode(&data))]);
        let m = from_compiled(&b).unwrap();
        assert!(m.compiled);
        assert_eq!(m.texture("g_tColor"), Some("materials/x/a_color_png_1.vtex"));
        assert_eq!(m.vector("g_vTint"), Some([1.0, 0.5, 0.0, 0.0]));
        assert_eq!(m.inputs, ["materials/x/a_color.png"], "Valve's defaults are left out");
        assert!(from_compiled(&crate::synth_with(&[], &[(*b"DATA", crate::kv3::encode(&Value::Object(vec![])))])).is_err());
    }

    #[test]
    fn broken_text_is_an_error_not_a_panic() {
        for t in ["\"Layer0\" { \"shader\" ", "\"Layer0\" {", "{", "}", "\"a\"", "\"Layer0\" { \"x\" \"1\" }", ""] {
            let _ = from_source(t);
        }
        assert!(from_source("\"Layer0\" { \"x\" \"1\" }").is_err());
        assert_eq!(parse_vector("[1, 2,3]"), Some([1.0, 2.0, 3.0, 0.0]));
        assert_eq!(parse_vector("1 2"), None);
    }
}
