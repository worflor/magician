//! Just enough glTF-binary reading to prove the native decode against Source 2 Viewer's export:
//! a JSON parser, accessors (float, normalised and integer component types, byte strides) and the
//! node tree's world matrices. Not a general glTF loader.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

impl Json {
    pub fn get(&self, k: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.get(k),
            _ => None,
        }
    }
    pub fn at(&self, i: usize) -> Option<&Json> {
        match self {
            Json::Arr(a) => a.get(i),
            _ => None,
        }
    }
    pub fn arr(&self) -> &[Json] {
        match self {
            Json::Arr(a) => a,
            _ => &[],
        }
    }
    pub fn num(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }
    pub fn str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
}

pub fn parse_json(s: &str) -> Result<Json, String> {
    let b = s.as_bytes();
    let mut p = 0;
    let v = value(b, &mut p)?;
    ws(b, &mut p);
    if p != b.len() {
        return Err(format!("trailing JSON at byte {p}"));
    }
    Ok(v)
}

fn ws(b: &[u8], p: &mut usize) {
    while *p < b.len() && b[*p].is_ascii_whitespace() {
        *p += 1;
    }
}

fn value(b: &[u8], p: &mut usize) -> Result<Json, String> {
    ws(b, p);
    let c = *b.get(*p).ok_or("JSON ended early")?;
    match c {
        b'{' => {
            *p += 1;
            let mut m = BTreeMap::new();
            ws(b, p);
            if b.get(*p) == Some(&b'}') {
                *p += 1;
                return Ok(Json::Obj(m));
            }
            loop {
                ws(b, p);
                let k = match value(b, p)? {
                    Json::Str(s) => s,
                    _ => return Err("object key is not a string".into()),
                };
                ws(b, p);
                if b.get(*p) != Some(&b':') {
                    return Err(format!("expected ':' at {p}"));
                }
                *p += 1;
                let v = value(b, p)?;
                m.insert(k, v);
                ws(b, p);
                match b.get(*p) {
                    Some(b',') => *p += 1,
                    Some(b'}') => {
                        *p += 1;
                        return Ok(Json::Obj(m));
                    }
                    _ => return Err(format!("expected ',' or '}}' at {p}")),
                }
            }
        }
        b'[' => {
            *p += 1;
            let mut a = Vec::new();
            ws(b, p);
            if b.get(*p) == Some(&b']') {
                *p += 1;
                return Ok(Json::Arr(a));
            }
            loop {
                a.push(value(b, p)?);
                ws(b, p);
                match b.get(*p) {
                    Some(b',') => *p += 1,
                    Some(b']') => {
                        *p += 1;
                        return Ok(Json::Arr(a));
                    }
                    _ => return Err(format!("expected ',' or ']' at {p}")),
                }
            }
        }
        b'"' => {
            *p += 1;
            let mut s = String::new();
            loop {
                let c = *b.get(*p).ok_or("unterminated string")?;
                *p += 1;
                match c {
                    b'"' => return Ok(Json::Str(s)),
                    b'\\' => {
                        let e = *b.get(*p).ok_or("bad escape")?;
                        *p += 1;
                        match e {
                            b'n' => s.push('\n'),
                            b't' => s.push('\t'),
                            b'r' => s.push('\r'),
                            b'b' => s.push('\u{8}'),
                            b'f' => s.push('\u{c}'),
                            b'u' => {
                                let h = std::str::from_utf8(b.get(*p..*p + 4).ok_or("bad \\u")?).map_err(|e| e.to_string())?;
                                let cp = u32::from_str_radix(h, 16).map_err(|e| e.to_string())?;
                                *p += 4;
                                s.push(char::from_u32(cp).unwrap_or('?'));
                            }
                            other => s.push(other as char),
                        }
                    }
                    _ => {
                        // copy the UTF-8 run
                        let start = *p - 1;
                        let mut end = *p;
                        while end < b.len() && b[end] != b'"' && b[end] != b'\\' {
                            end += 1;
                        }
                        s.push_str(std::str::from_utf8(&b[start..end]).map_err(|e| e.to_string())?);
                        *p = end;
                    }
                }
            }
        }
        b't' if b[*p..].starts_with(b"true") => {
            *p += 4;
            Ok(Json::Bool(true))
        }
        b'f' if b[*p..].starts_with(b"false") => {
            *p += 5;
            Ok(Json::Bool(false))
        }
        b'n' if b[*p..].starts_with(b"null") => {
            *p += 4;
            Ok(Json::Null)
        }
        _ => {
            let start = *p;
            while *p < b.len() && matches!(b[*p], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                *p += 1;
            }
            let t = std::str::from_utf8(&b[start..*p]).map_err(|e| e.to_string())?;
            t.parse::<f64>().map(Json::Num).map_err(|_| format!("bad number '{t}' at {start}"))
        }
    }
}

pub struct Glb {
    pub json: Json,
    pub bin: Vec<u8>,
}

impl Glb {
    pub fn read(bytes: &[u8]) -> Result<Glb, String> {
        if bytes.len() < 20 || &bytes[0..4] != b"glTF" {
            return Err("not a glb".into());
        }
        let mut p = 12;
        let mut json = None;
        let mut bin = Vec::new();
        while p + 8 <= bytes.len() {
            let len = u32::from_le_bytes([bytes[p], bytes[p + 1], bytes[p + 2], bytes[p + 3]]) as usize;
            let kind = &bytes[p + 4..p + 8];
            let body = bytes.get(p + 8..p + 8 + len).ok_or("glb chunk past the end")?;
            if kind == b"JSON" {
                json = Some(parse_json(std::str::from_utf8(body).map_err(|e| e.to_string())?)?);
            } else if kind == b"BIN\0" {
                bin = body.to_vec();
            }
            p += 8 + len;
        }
        Ok(Glb { json: json.ok_or("glb has no JSON chunk")?, bin })
    }

    /// An accessor's values as floats (normalised integer types scaled), `n` components each.
    pub fn accessor(&self, i: usize) -> Result<(usize, Vec<f32>), String> {
        let a = self.json.get("accessors").and_then(|x| x.at(i)).ok_or("accessor out of range")?;
        let count = a.get("count").and_then(Json::num).unwrap_or(0.0) as usize;
        let n = match a.get("type").and_then(Json::str).unwrap_or("") {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT4" => 16,
            t => return Err(format!("accessor type {t}")),
        };
        let ct = a.get("componentType").and_then(Json::num).unwrap_or(0.0) as u32;
        let norm = matches!(a.get("normalized"), Some(Json::Bool(true)));
        let csize = match ct {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return Err(format!("component type {ct}")),
        };
        let bv_i = a.get("bufferView").and_then(Json::num).ok_or("sparse/empty accessor")? as usize;
        let bv = self.json.get("bufferViews").and_then(|x| x.at(bv_i)).ok_or("bufferView out of range")?;
        // offsets, strides and counts are the file's numbers: every step is checked, and past the buffer is an error
        let base = (bv.get("byteOffset").and_then(Json::num).unwrap_or(0.0) as usize).checked_add(a.get("byteOffset").and_then(Json::num).unwrap_or(0.0) as usize).ok_or("accessor offset out of range")?;
        let stride = bv.get("byteStride").and_then(Json::num).map(|x| x as usize).unwrap_or(n * csize);
        let mut out = Vec::with_capacity(count.min(self.bin.len()).saturating_mul(n));
        for e in 0..count {
            for c in 0..n {
                let at = e.checked_mul(stride).and_then(|x| x.checked_add(base)).and_then(|x| x.checked_add(c * csize)).ok_or("accessor past the buffer")?;
                let b = self.bin.get(at..at.saturating_add(csize)).ok_or("accessor past the buffer")?;
                let v = match ct {
                    5126 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                    5125 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
                    5123 => {
                        let x = u16::from_le_bytes([b[0], b[1]]) as f32;
                        if norm { x / 65535.0 } else { x }
                    }
                    5122 => {
                        let x = i16::from_le_bytes([b[0], b[1]]) as f32;
                        if norm { (x / 32767.0).max(-1.0) } else { x }
                    }
                    5121 => {
                        let x = b[0] as f32;
                        if norm { x / 255.0 } else { x }
                    }
                    _ => {
                        let x = b[0] as i8 as f32;
                        if norm { (x / 127.0).max(-1.0) } else { x }
                    }
                };
                out.push(v);
            }
        }
        Ok((n, out))
    }

    /// Accessor indices as u32 (index buffers).
    pub fn indices(&self, i: usize) -> Result<Vec<u32>, String> {
        Ok(self.accessor(i)?.1.into_iter().map(|x| x as u32).collect())
    }
}
