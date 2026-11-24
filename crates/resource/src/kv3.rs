//! KeyValues3: the data inside compiled resources (`DATA`, `CTRL`, `PHYS`, `ASEQ`, `AGRP`, `MDAT` blocks of
//! particles, models, sound events, vdata, materials...), read natively.
//!
//! Binary forms read: KV3 v4 (`\x04 3VK`) and v5 (`\x05 3VK`), uncompressed, LZ4 or zstd (`zstd.rs`; a v5 block
//! keeps both buffers and the blobs in one zstd run: the map entity lumps, `maps/<map>/entities/*.vents_c`).
//! A survey of build 6864 (3,000 random game files) found nothing else in the blocks lint reads; zstd also
//! appears in some model `ANIM` blocks, which are compared as bytes, never decoded. The text form (what Source 2 Viewer
//! decompiles to, and how fixtures are written) is parsed too.
//!
//! v5 layout, after the magic: format guid (16), compression method u32, dictionary id u16, frame size u16,
//! then counts; the payload is two buffers. Buffer 0 holds the string table (its bytes) and the string
//! count (its first int). Buffer 1 holds one u32 member count per object, then the value streams (bytes,
//! shorts, ints, doubles, each aligned to its size), then one type byte per value (plus a specifier byte
//! when bit 0x80 is set), then 0xFFEEDD00. Values are read depth-first in a single pass.

use crate::lz4;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Double(f64),
    Str(String),
    Blob(Vec<u8>),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    /// `a.b.c` through objects; a numeric segment indexes an array.
    pub fn at(&self, path: &str) -> Option<&Value> {
        let mut v = self;
        for seg in path.split('.') {
            v = match v {
                Value::Array(a) => a.get(seg.parse::<usize>().ok()?)?,
                _ => v.get(seg)?,
            };
        }
        Some(v)
    }
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Value::Int(i) => Some(i as f64),
            Value::UInt(u) => Some(u as f64),
            Value::Double(d) => Some(d),
            Value::Bool(b) => Some(b as u8 as f64),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            Value::Bool(b) => Some(b),
            Value::Int(i) => Some(i != 0),
            Value::UInt(u) => Some(u != 0),
            _ => None,
        }
    }
    pub fn as_array(&self) -> &[Value] {
        match self {
            Value::Array(a) => a,
            _ => &[],
        }
    }
    pub fn members(&self) -> &[(String, Value)] {
        match self {
            Value::Object(m) => m,
            _ => &[],
        }
    }
    /// The numbers of an array value (a colour, a vector), empty if it isn't one.
    pub fn numbers(&self) -> Vec<f64> {
        self.as_array().iter().filter_map(Value::as_f64).collect()
    }
    /// Visit every value depth-first with the key it sits under (`""` for array items and the root).
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a str, &'a Value)) {
        fn go<'a>(key: &'a str, v: &'a Value, f: &mut dyn FnMut(&'a str, &'a Value)) {
            f(key, v);
            match v {
                Value::Array(a) => a.iter().for_each(|x| go("", x, f)),
                Value::Object(m) => m.iter().for_each(|(k, x)| go(k, x, f)),
                _ => {}
            }
        }
        go("", self, f)
    }
    /// Every string anywhere in the value.
    pub fn strings(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.walk(&mut |_, v| {
            if let Value::Str(s) = v {
                out.push(s.as_str());
            }
        });
        out
    }

    /// Text form (KV3 text, close to what Source 2 Viewer writes), for dumps and diffs.
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        write_text(self, 0, &mut s);
        s.push('\n');
        s
    }
}

fn write_text(v: &Value, depth: usize, s: &mut String) {
    let ind = |n: usize, s: &mut String| (0..n).for_each(|_| s.push('\t'));
    match v {
        Value::Null => s.push_str("null"),
        Value::Bool(b) => s.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => s.push_str(&i.to_string()),
        Value::UInt(u) => s.push_str(&u.to_string()),
        Value::Double(d) => {
            if d.fract() == 0.0 && d.abs() < 1e15 {
                s.push_str(&format!("{d:.1}"))
            } else {
                s.push_str(&format!("{d}"))
            }
        }
        Value::Str(x) => {
            s.push('"');
            for c in x.chars() {
                match c {
                    '"' => s.push_str("\\\""),
                    '\\' => s.push_str("\\\\"),
                    '\n' => s.push_str("\\n"),
                    c => s.push(c),
                }
            }
            s.push('"');
        }
        Value::Blob(b) => {
            s.push_str("#[");
            for (i, x) in b.iter().enumerate() {
                if i > 0 {
                    s.push(' ');
                }
                s.push_str(&format!("{x:02X}"));
            }
            s.push(']');
        }
        Value::Array(a) => {
            if a.iter().all(|x| !matches!(x, Value::Array(_) | Value::Object(_))) && a.len() <= 16 {
                s.push('[');
                for (i, x) in a.iter().enumerate() {
                    s.push_str(if i == 0 { " " } else { ", " });
                    write_text(x, depth, s);
                }
                s.push_str(" ]");
            } else {
                s.push('\n');
                ind(depth, s);
                s.push_str("[\n");
                for x in a {
                    ind(depth + 1, s);
                    write_text(x, depth + 1, s);
                    s.push_str(",\n");
                }
                ind(depth, s);
                s.push(']');
            }
        }
        Value::Object(m) => {
            if depth > 0 {
                s.push('\n');
                ind(depth, s);
            }
            s.push_str("{\n");
            for (k, x) in m {
                ind(depth + 1, s);
                if k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') && !k.is_empty() {
                    s.push_str(k);
                } else {
                    write_text(&Value::Str(k.clone()), depth, s);
                }
                s.push_str(" = ");
                write_text(x, depth + 1, s);
                s.push('\n');
            }
            ind(depth, s);
            s.push('}');
        }
    }
}

// ---------------------------------------------------------------------------------------------- binary

const SENTINEL: u32 = 0xFFEE_DD00;

struct Stream<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Stream<'a> {
    fn new(b: &'a [u8]) -> Self {
        Stream { b, p: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let s = self.b.get(self.p..self.p + n).ok_or_else(|| format!("kv3: read of {n} bytes past end at {}", self.p))?;
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Result<u32, String> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn u64(&mut self) -> Result<u64, String> {
        let s = self.take(8)?;
        Ok(u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
    }
    /// Bytes not read yet: the most elements a count read from the file can have, at one byte each.
    fn left(&self) -> usize {
        self.b.len().saturating_sub(self.p)
    }
    fn align(&mut self, n: usize) {
        self.p = self.p.div_ceil(n) * n;
    }
    fn cstr(&mut self) -> Result<String, String> {
        let rest = &self.b[self.p.min(self.b.len())..];
        let end = rest.iter().position(|&c| c == 0).ok_or("kv3: unterminated string")?;
        self.p += end + 1;
        Ok(String::from_utf8_lossy(&rest[..end]).into_owned())
    }
}

/// The four value streams of one buffer.
struct Streams<'a> {
    bytes: Stream<'a>,
    shorts: Stream<'a>,
    ints: Stream<'a>,
    doubles: Stream<'a>,
}

/// Split a buffer into its streams starting at `s.p`, aligning each to its element size (relative to the
/// buffer's start), and leave `s.p` after the doubles.
fn split<'a>(s: &mut Stream<'a>, nb: usize, ns: usize, ni: usize, nd: usize, force_align: bool) -> Result<Streams<'a>, String> {
    let bytes = Stream::new(s.take(nb)?);
    if ns > 0 || force_align {
        s.align(2);
    }
    let shorts = Stream::new(s.take(ns * 2)?);
    if ni > 0 || force_align {
        s.align(4);
    }
    let ints = Stream::new(s.take(ni * 4)?);
    if nd > 0 || force_align {
        s.align(8);
    }
    let doubles = Stream::new(s.take(nd * 8)?);
    Ok(Streams { bytes, shorts, ints, doubles })
}

struct Ctx<'a> {
    strings: Vec<String>,
    b0: Streams<'a>,
    b1: Streams<'a>,
    /// v5 keeps object member counts in their own stream; v4 reads them from the int stream.
    counts: Option<Stream<'a>>,
    types: Vec<u8>,
    tp: usize,
    blobs: Option<(Vec<u32>, Vec<u8>, usize)>,
    use_b0: bool,
    /// v4 has one buffer, so "read from buffer 0" (type 25) reads the same streams.
    single: bool,
    /// Elements typed arrays may still hold. A typed array of constants (null, true, 0, 1.0) costs no bytes per
    /// element, so its length, a number in the file, is bounded by this instead.
    budget: usize,
}

/// How many typed-array elements a document whose buffers hold `bytes` may decode to: far more than any real file
/// (they hold a few constants per byte at most), far less than memory.
fn element_budget(bytes: usize) -> usize {
    bytes.saturating_mul(16).max(1 << 20)
}

impl<'a> Ctx<'a> {
    fn cur(&mut self) -> &mut Streams<'a> {
        if self.use_b0 { &mut self.b0 } else { &mut self.b1 }
    }
    fn ty(&mut self) -> Result<u8, String> {
        let t = *self.types.get(self.tp).ok_or("kv3: type stream exhausted")?;
        self.tp += 1;
        if t & 0x80 != 0 {
            self.tp += 1; // specifier (resource, soundevent, panorama...): not needed to read the value
        }
        Ok(t & 0x3F)
    }
    fn string(&mut self) -> Result<String, String> {
        let id = self.cur().ints.u32()? as i32;
        if id < 0 {
            return Ok(String::new());
        }
        self.strings.get(id as usize).cloned().ok_or_else(|| format!("kv3: string id {id} out of range"))
    }
    fn value(&mut self) -> Result<Value, String> {
        let t = self.ty()?;
        self.typed(t)
    }
    fn typed(&mut self, t: u8) -> Result<Value, String> {
        Ok(match t {
            1 => Value::Null,
            2 => Value::Bool(self.cur().bytes.u8()? == 1),
            3 => Value::Int(self.cur().doubles.u64()? as i64),
            4 => Value::UInt(self.cur().doubles.u64()?),
            5 => Value::Double(f64::from_bits(self.cur().doubles.u64()?)),
            6 => Value::Str(self.string()?),
            7 => match &mut self.blobs {
                Some((sizes, data, at)) => {
                    let n = if sizes.is_empty() { 0 } else { sizes.remove(0) as usize };
                    let b = data.get(*at..*at + n).ok_or("kv3: blob past end")?.to_vec();
                    *at += n;
                    Value::Blob(b)
                }
                None => {
                    let n = self.cur().ints.u32()? as usize;
                    Value::Blob(self.cur().bytes.take(n)?.to_vec())
                }
            },
            8 => {
                let n = self.cur().ints.u32()? as usize;
                let mut a = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    a.push(self.value()?);
                }
                Value::Array(a)
            }
            9 => {
                let n = match &mut self.counts {
                    Some(c) => c.u32()?,
                    None => self.cur().ints.u32()?,
                } as usize;
                let mut m = Vec::with_capacity(n.min(1 << 16));
                for i in 0..n {
                    let id = self.cur().ints.u32()? as i32;
                    let k = if id < 0 { i.to_string() } else { self.strings.get(id as usize).cloned().ok_or("kv3: bad member name")? };
                    let v = self.value()?;
                    m.push((k, v));
                }
                Value::Object(m)
            }
            10 => {
                let n = self.cur().ints.u32()? as usize;
                self.typed_array(n)?
            }
            11 => Value::Int(self.cur().ints.u32()? as i32 as i64),
            12 => Value::UInt(self.cur().ints.u32()? as u64),
            13 => Value::Bool(true),
            14 => Value::Bool(false),
            15 => Value::Int(0),
            16 => Value::Int(1),
            17 => Value::Double(0.0),
            18 => Value::Double(1.0),
            19 => Value::Double(f32::from_bits(self.cur().ints.u32()?) as f64),
            20 => Value::Int(self.cur().shorts.u16()? as i16 as i64),
            21 => Value::UInt(self.cur().shorts.u16()? as u64),
            22 => Value::Int(self.cur().bytes.u8()? as i8 as i64),
            23 => Value::UInt(self.cur().bytes.u8()? as u64),
            24 => {
                let n = self.cur().bytes.u8()? as usize;
                self.typed_array(n)?
            }
            25 => {
                let n = self.cur().bytes.u8()? as usize;
                let was = self.use_b0;
                self.use_b0 = !self.single;
                let a = self.typed_array(n);
                self.use_b0 = was;
                a?
            }
            26 => Value::UInt(0),
            27 => Value::UInt(1),
            _ => return Err(format!("kv3: unknown type {t}")),
        })
    }
    fn typed_array(&mut self, n: usize) -> Result<Value, String> {
        let et = self.ty()?;
        self.budget = self.budget.checked_sub(n).ok_or_else(|| format!("kv3: a typed array of {n} elements, more than the data could hold"))?;
        let mut a = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            a.push(self.typed(et)?);
        }
        Ok(Value::Array(a))
    }
}

/// Decode a binary KV3 blob (a block's bytes, starting at the magic).
pub fn decode(b: &[u8]) -> Result<Value, String> {
    if b.len() < 4 {
        return Err("kv3: too short".into());
    }
    match (&b[1..4], b[0]) {
        (b"3VK", 5) => decode_v5(b),
        (b"3VK", 4) => decode_v4(b),
        (b"3VK", v) => Err(format!("kv3: version {v} not supported")),
        _ if &b[..4] == b"VKV\x03" => Err("kv3: legacy VKV3 not supported".into()),
        _ => Err("kv3: not a KV3 block".into()),
    }
}

/// True when the bytes start with a KV3 magic this reader knows.
pub fn is_kv3(b: &[u8]) -> bool {
    b.len() >= 4 && &b[1..4] == b"3VK" && (b[0] == 4 || b[0] == 5)
}

fn unpack(method: u32, data: &[u8], size: usize) -> Result<Vec<u8>, String> {
    match method {
        0 => Ok(data.get(..size).ok_or("kv3: buffer past end")?.to_vec()),
        1 => lz4::decompress(data, size),
        2 => crate::zstd::decompress_exact(data, size),
        m => Err(format!("kv3: unknown compression {m}")),
    }
}

fn chained_blobs(method: u32, frame: usize, sizes: &[u32], comp_sizes: &[u16], s: &mut Stream) -> Result<Vec<u8>, String> {
    let total: usize = sizes.iter().map(|&x| x as usize).sum();
    if method == 0 {
        return Ok(s.take(total)?.to_vec());
    }
    // the sizes are the file's word: reserve no more than the frames left could decode to
    let mut out = Vec::with_capacity(total.min(s.left().saturating_mul(255)));
    let mut ci = 0;
    for &bs in sizes {
        let mut left = bs as usize;
        while left > 0 {
            let n = *comp_sizes.get(ci).ok_or("kv3: missing blob frame size")? as usize;
            ci += 1;
            let before = out.len();
            lz4::decompress_into(s.take(n)?, &mut out)?;
            let keep = frame.min(left);
            if out.len() - before < keep {
                return Err("kv3: blob frame decoded short".into());
            }
            out.truncate(before + keep);
            left -= keep;
        }
    }
    Ok(out)
}

fn decode_v5(b: &[u8]) -> Result<Value, String> {
    let mut s = Stream::new(b);
    s.take(4 + 16)?;
    let method = s.u32()?;
    let _dict = s.u16()?;
    let frame = s.u16()? as usize;
    let (nb, ni, nd) = (s.u32()? as usize, s.u32()? as usize, s.u32()? as usize);
    let types_size = s.u32()? as usize;
    let (_oc, _ac) = (s.u16()?, s.u16()?);
    let (_unc_total, comp_total) = (s.u32()? as usize, s.u32()? as usize);
    let (block_count, block_total) = (s.u32()? as usize, s.u32()? as usize);
    let ns = s.u32()? as usize;
    let comp_block_sizes = s.u32()? as usize / 2;
    let (b0_size, b0_comp) = (s.u32()? as usize, s.u32()? as usize);
    let (b1_size, b1_comp) = (s.u32()? as usize, s.u32()? as usize);
    let (nb2, ns2, ni2, nd2) = (s.u32()? as usize, s.u32()? as usize, s.u32()? as usize, s.u32()? as usize);
    let (_f54, object_count, _array_count, _f60) = (s.u32()?, s.u32()? as usize, s.u32()?, s.u32()?);
    // zstd (method 2): buffer 0, buffer 1 and the blobs are one compressed run of comp_total bytes (one frame or
    // several back to back), decoding to the three one after another (entity lumps, 6864)
    let mut zblobs: Option<Vec<u8>> = None;
    let (buf0, buf1) = if method == 2 {
        let all = crate::zstd::decompress(s.take(comp_total)?)?;
        if all.len() < b0_size + b1_size {
            return Err(format!("kv3: zstd run decoded {} bytes, buffers need {}", all.len(), b0_size + b1_size));
        }
        zblobs = Some(all[b0_size + b1_size..].to_vec());
        (all[..b0_size].to_vec(), all[b0_size..b0_size + b1_size].to_vec())
    } else {
        let (c0, c1) = if method == 0 { (b0_size, b1_size) } else { (b0_comp, b1_comp) };
        let raw0 = s.take(c0)?;
        let raw1 = s.take(c1)?;
        (unpack(method, raw0, b0_size)?, unpack(method, raw1, b1_size)?)
    };

    let mut s0 = Stream::new(&buf0);
    let mut b0 = split(&mut s0, nb, ns, ni, nd, false)?;
    let nstr = b0.ints.u32()? as usize;
    let mut strings = Vec::with_capacity(nstr.min(b0.bytes.left()));
    for _ in 0..nstr {
        strings.push(b0.bytes.cstr()?);
    }
    let mut s1 = Stream::new(&buf1);
    let counts = Stream::new(s1.take(object_count * 4)?);
    let b1 = split(&mut s1, nb2, ns2, ni2, nd2, false)?;
    let types = s1.take(types_size)?.to_vec();
    let blobs = if block_count == 0 {
        if s1.u32()? != SENTINEL {
            return Err("kv3: missing end marker".into());
        }
        None
    } else {
        let mut sizes = Vec::with_capacity(block_count.min(s1.left() / 4));
        for _ in 0..block_count {
            sizes.push(s1.u32()?);
        }
        if s1.u32()? != SENTINEL {
            return Err("kv3: missing end marker after blob sizes".into());
        }
        let mut data = Vec::new();
        if block_total > 0 {
            match method {
                0 | 1 => {
                    let mut cs = Vec::with_capacity(comp_block_sizes.min(s1.left() / 2));
                    for _ in 0..comp_block_sizes {
                        cs.push(s1.u16()?);
                    }
                    data = chained_blobs(method, frame.max(1), &sizes, &cs, &mut s)?;
                }
                _ => {
                    let z = zblobs.take().unwrap_or_default();
                    if z.len() < block_total {
                        return Err(format!("kv3: zstd blobs decoded {} bytes, the sizes need {block_total}", z.len()));
                    }
                    data = z[..block_total].to_vec();
                }
            }
        }
        Some((sizes, data, 0))
    };
    let budget = element_budget(buf0.len() + buf1.len() + blobs.as_ref().map_or(0, |b| b.1.len()));
    let mut c = Ctx { strings, b0, b1, counts: Some(counts), types, tp: 0, blobs, use_b0: false, single: false, budget };
    c.value()
}

fn decode_v4(b: &[u8]) -> Result<Value, String> {
    let mut s = Stream::new(b);
    s.take(4 + 16)?;
    let method = s.u32()?;
    let _dict = s.u16()?;
    let frame = s.u16()? as usize;
    let (nb, ni, nd) = (s.u32()? as usize, s.u32()? as usize, s.u32()? as usize);
    let st_size = s.u32()? as usize;
    let (_oc, _ac) = (s.u16()?, s.u16()?);
    let (unc, comp) = (s.u32()? as usize, s.u32()? as usize);
    let (block_count, block_total) = (s.u32()? as usize, s.u32()? as usize);
    let ns = s.u32()? as usize;
    let _comp_block_sizes = s.u32()? as usize / 2;
    let buf = match method {
        0 => s.take(comp)?.to_vec(),
        _ => unpack(method, s.take(comp)?, unc)?,
    };
    let mut d = Stream::new(&buf);
    let mut st = split(&mut d, nb, ns, ni, nd, true)?;
    let strings_and_types = d.take(st_size)?;
    let mut sts = Stream::new(strings_and_types);
    let nstr = st.ints.u32()? as usize;
    let mut strings = Vec::with_capacity(nstr.min(sts.left()));
    for _ in 0..nstr {
        strings.push(sts.cstr()?);
    }
    let types = strings_and_types[sts.p..].to_vec();
    let blobs = if block_count == 0 {
        None
    } else {
        let mut sizes = Vec::with_capacity(block_count.min(d.left() / 4));
        for _ in 0..block_count {
            sizes.push(d.u32()?);
        }
        if d.u32()? != SENTINEL {
            return Err("kv3: missing end marker after blob sizes".into());
        }
        let mut data = Vec::new();
        if block_total > 0 {
            let mut cs = Vec::new();
            while d.p + 2 <= d.b.len() {
                cs.push(d.u16()?);
            }
            data = chained_blobs(method, frame.max(1), &sizes, &cs, &mut s)?;
        }
        Some((sizes, data, 0))
    };
    let empty = Streams { bytes: Stream::new(&[]), shorts: Stream::new(&[]), ints: Stream::new(&[]), doubles: Stream::new(&[]) };
    let budget = element_budget(buf.len() + blobs.as_ref().map_or(0, |b| b.1.len()));
    let mut c = Ctx { strings, b0: empty, b1: st, counts: None, types, tp: 0, blobs, use_b0: false, single: true, budget };
    c.value()
}

/// Write `v` as uncompressed binary KV3 v5, in the layout Valve's files (and so Source 2 Viewer's reader) use. Lint
/// fixtures use it to build data blocks the reader must accept exactly as it accepts the compiler's, and
/// `tools/scene_lump.py` writes the same layout for UI scene entity lumps.
///
/// Blobs (`#[...]`, e.g. every entity's empty `m_keyValuesData`) are not stored inline. They live in blocks: the
/// header carries the block count and total bytes, buffer 1 ends with one u32 size per blob and then 0xFFEEDD00, and
/// the blob bytes follow the two buffers in the file, **closed by a second 0xFFEEDD00**. Without the second marker
/// Source 2 Viewer stops at end of stream; blobs inline with no blocks index out of range. Magician's own reader
/// accepts all three, so a round trip through it proves nothing: `blobs_live_in_blocks_in_the_layout_the_game_uses`
/// pins the bytes instead, and the layout was checked against Source 2 Viewer 21.1 on a real entity lump.
pub fn encode(v: &Value) -> Vec<u8> {
    struct W {
        strings: Vec<String>,
        counts: Vec<u8>,
        ints: Vec<u8>,
        doubles: Vec<u8>,
        types: Vec<u8>,
        blobs: Vec<Vec<u8>>,
        objects: u32,
        arrays: u32,
        members: u32,
    }
    impl W {
        fn sid(&mut self, s: &str) -> i32 {
            if s.is_empty() {
                return -1;
            }
            match self.strings.iter().position(|x| x == s) {
                Some(i) => i as i32,
                None => {
                    self.strings.push(s.to_string());
                    self.strings.len() as i32 - 1
                }
            }
        }
        fn val(&mut self, v: &Value) {
            match v {
                Value::Null => self.types.push(1),
                Value::Bool(b) => self.types.push(if *b { 13 } else { 14 }),
                Value::Int(0) => self.types.push(15),
                Value::Int(1) => self.types.push(16),
                Value::Int(i) => {
                    self.types.push(3);
                    self.doubles.extend_from_slice(&i.to_le_bytes());
                }
                Value::UInt(u) => {
                    self.types.push(4);
                    self.doubles.extend_from_slice(&u.to_le_bytes());
                }
                Value::Double(d) if *d == 0.0 => self.types.push(17),
                Value::Double(d) if *d == 1.0 => self.types.push(18),
                Value::Double(d) => {
                    self.types.push(5);
                    self.doubles.extend_from_slice(&d.to_bits().to_le_bytes());
                }
                Value::Str(s) => {
                    self.types.push(6);
                    let id = self.sid(s);
                    self.ints.extend_from_slice(&id.to_le_bytes());
                }
                Value::Blob(b) => {
                    self.types.push(7);
                    self.blobs.push(b.clone());
                }
                Value::Array(a) => {
                    self.arrays += 1;
                    self.types.push(8);
                    self.ints.extend_from_slice(&(a.len() as u32).to_le_bytes());
                    a.iter().for_each(|x| self.val(x));
                }
                Value::Object(m) => {
                    self.objects += 1;
                    self.members += m.len() as u32;
                    self.types.push(9);
                    self.counts.extend_from_slice(&(m.len() as u32).to_le_bytes());
                    for (k, x) in m {
                        let id = self.sid(k);
                        self.ints.extend_from_slice(&id.to_le_bytes());
                        self.val(x);
                    }
                }
            }
        }
    }
    let mut w = W { strings: vec![], counts: vec![], ints: vec![], doubles: vec![], types: vec![], blobs: vec![], objects: 0, arrays: 0, members: 0 };
    w.val(v);
    let pad = |b: &mut Vec<u8>, n: usize| {
        while !b.len().is_multiple_of(n) {
            b.push(0)
        }
    };
    // buffer 0: string bytes, then the string count as its only int
    let mut b0 = Vec::new();
    for s in &w.strings {
        b0.extend_from_slice(s.as_bytes());
        b0.push(0);
    }
    let nb0 = b0.len();
    pad(&mut b0, 4);
    b0.extend_from_slice(&(w.strings.len() as u32).to_le_bytes());
    // buffer 1: member counts, ints (aligned 4), doubles (aligned 8), types, blob sizes, end marker
    let mut b1 = w.counts.clone();
    let ni = w.ints.len() / 4;
    let nd = w.doubles.len() / 8;
    if ni > 0 {
        pad(&mut b1, 4);
    }
    b1.extend_from_slice(&w.ints);
    if nd > 0 {
        pad(&mut b1, 8);
    }
    b1.extend_from_slice(&w.doubles);
    b1.extend_from_slice(&w.types);
    for b in &w.blobs {
        b1.extend_from_slice(&(b.len() as u32).to_le_bytes());
    }
    b1.extend_from_slice(&SENTINEL.to_le_bytes());
    // the blob data follows the buffers and ends on its own marker
    let blob_total: usize = w.blobs.iter().map(Vec::len).sum();
    let mut blob_data: Vec<u8> = w.blobs.concat();
    if !w.blobs.is_empty() {
        blob_data.extend_from_slice(&SENTINEL.to_le_bytes());
    }

    let mut o = Vec::new();
    let u32le = |o: &mut Vec<u8>, x: usize| o.extend_from_slice(&(x as u32).to_le_bytes());
    o.extend_from_slice(b"\x053VK");
    o.extend_from_slice(&[0x7c, 0x16, 0x12, 0x74, 0xe9, 0x06, 0x98, 0x46, 0xaf, 0xf2, 0xe6, 0x3e, 0xb5, 0x90, 0x37, 0xe7]);
    u32le(&mut o, 0); // uncompressed
    o.extend_from_slice(&0u16.to_le_bytes());
    o.extend_from_slice(&0u16.to_le_bytes());
    u32le(&mut o, nb0);
    u32le(&mut o, 1);
    u32le(&mut o, 0);
    u32le(&mut o, w.types.len());
    o.extend_from_slice(&(w.objects as u16).to_le_bytes());
    o.extend_from_slice(&(w.arrays as u16).to_le_bytes());
    u32le(&mut o, b0.len() + b1.len());
    u32le(&mut o, b0.len() + b1.len());
    u32le(&mut o, w.blobs.len()); // blocks
    u32le(&mut o, blob_total);
    u32le(&mut o, 0); // shorts in buffer 0
    u32le(&mut o, 0);
    u32le(&mut o, b0.len());
    u32le(&mut o, b0.len());
    u32le(&mut o, b1.len());
    u32le(&mut o, b1.len());
    u32le(&mut o, 0); // bytes in buffer 1
    u32le(&mut o, 0);
    u32le(&mut o, ni);
    u32le(&mut o, nd);
    u32le(&mut o, w.members as usize);
    u32le(&mut o, w.objects as usize);
    u32le(&mut o, w.arrays as usize);
    u32le(&mut o, 0);
    o.extend_from_slice(&b0);
    o.extend_from_slice(&b1);
    o.extend_from_slice(&blob_data);
    o
}

// ------------------------------------------------------------------------------------------------ text

/// Parse KV3 text (Source 2 Viewer's decompiled form, or a hand-written fixture). The `<!-- kv3 ... -->`
/// header is optional. Flags (`resource_name:"..."`, `soundevent:"..."`) are dropped; the value is kept.
pub fn parse_text(src: &str) -> Result<Value, String> {
    let mut p = TextParser { s: src.as_bytes(), i: 0 };
    p.ws();
    if p.s[p.i..].starts_with(b"<!--") {
        let end = p.s[p.i..].windows(3).position(|w| w == b"-->").ok_or("kv3 text: unterminated header")?;
        p.i += end + 3;
    }
    p.ws();
    let v = p.value()?;
    p.ws();
    Ok(v)
}

struct TextParser<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> TextParser<'a> {
    fn ws(&mut self) {
        loop {
            while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace()) {
                self.i += 1;
            }
            if self.s[self.i..].starts_with(b"//") {
                while self.i < self.s.len() && self.s[self.i] != b'\n' {
                    self.i += 1;
                }
            } else if self.s[self.i..].starts_with(b"/*") {
                match find(&self.s[self.i + 2..], b"*/") {
                    Some(e) => self.i += e + 4,
                    None => self.i = self.s.len(),
                }
            } else {
                break;
            }
        }
    }
    fn err<T>(&self, what: &str) -> Result<T, String> {
        let line = self.s[..self.i.min(self.s.len())].iter().filter(|&&c| c == b'\n').count() + 1;
        Err(format!("kv3 text line {line}: {what}"))
    }
    fn peek(&self) -> u8 {
        *self.s.get(self.i).unwrap_or(&0)
    }
    fn quoted(&mut self) -> Result<String, String> {
        if self.s[self.i..].starts_with(b"\"\"\"") {
            self.i += 3;
            let e = find(&self.s[self.i..], b"\"\"\"").ok_or("kv3 text: unterminated multi-line string")?;
            let t = String::from_utf8_lossy(&self.s[self.i..self.i + e]).trim_matches('\n').to_string();
            self.i += e + 3;
            return Ok(t);
        }
        self.i += 1;
        let mut out = Vec::new();
        while self.i < self.s.len() {
            let c = self.s[self.i];
            self.i += 1;
            match c {
                b'"' => return Ok(String::from_utf8_lossy(&out).into_owned()),
                b'\\' => {
                    let n = *self.s.get(self.i).unwrap_or(&b'\\');
                    self.i += 1;
                    out.push(match n {
                        b'n' => b'\n',
                        b't' => b'\t',
                        x => x,
                    });
                }
                x => out.push(x),
            }
        }
        self.err("unterminated string")
    }
    fn bare(&mut self) -> String {
        let st = self.i;
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-' | b'+' | b'$' | b'/' | b'@') {
                self.i += 1;
            } else {
                break;
            }
        }
        String::from_utf8_lossy(&self.s[st..self.i]).into_owned()
    }
    fn value(&mut self) -> Result<Value, String> {
        self.ws();
        match self.peek() {
            b'{' => {
                self.i += 1;
                let mut m = Vec::new();
                loop {
                    self.ws();
                    match self.peek() {
                        b'}' => {
                            self.i += 1;
                            return Ok(Value::Object(m));
                        }
                        0 => return self.err("unterminated object"),
                        b'"' => {
                            let k = self.quoted()?;
                            self.member(k, &mut m)?;
                        }
                        _ => {
                            let k = self.bare();
                            if k.is_empty() {
                                return self.err("expected a key");
                            }
                            self.member(k, &mut m)?;
                        }
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                loop {
                    self.ws();
                    match self.peek() {
                        b']' => {
                            self.i += 1;
                            return Ok(Value::Array(a));
                        }
                        b',' => self.i += 1,
                        0 => return self.err("unterminated array"),
                        _ => a.push(self.value()?),
                    }
                }
            }
            b'"' => Ok(Value::Str(self.quoted()?)),
            b'#' if self.s.get(self.i + 1) == Some(&b'[') => {
                self.i += 2;
                let mut b = Vec::new();
                loop {
                    self.ws();
                    if self.peek() == b']' {
                        self.i += 1;
                        return Ok(Value::Blob(b));
                    }
                    let h = self.bare();
                    b.push(u8::from_str_radix(&h, 16).map_err(|_| format!("kv3 text: bad blob byte '{h}'"))?);
                }
            }
            _ => {
                let w = self.bare();
                if self.peek() == b':' {
                    // a flag: resource:"x", soundevent:"y", subclass:{...}
                    self.i += 1;
                    return self.value();
                }
                match w.as_str() {
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    "null" => Ok(Value::Null),
                    "" => self.err("expected a value"),
                    _ => {
                        if let Ok(i) = w.parse::<i64>() {
                            Ok(Value::Int(i))
                        } else if let Ok(u) = w.parse::<u64>() {
                            Ok(Value::UInt(u))
                        } else if let Ok(d) = w.parse::<f64>() {
                            Ok(Value::Double(d))
                        } else {
                            Ok(Value::Str(w))
                        }
                    }
                }
            }
        }
    }
    fn member(&mut self, k: String, m: &mut Vec<(String, Value)>) -> Result<(), String> {
        self.ws();
        if self.peek() != b'=' {
            return self.err(&format!("expected '=' after '{k}'"));
        }
        self.i += 1;
        let v = self.value()?;
        m.push((k, v));
        Ok(())
    }
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = r#"<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} format:generic:version{7412167c-06e9-4698-aff2-e63eb59037e7} -->
{
	_class = "CParticleSystemDefinition"
	m_nMaxParticles = 16
	m_flScale = 0.85
	m_bOn = true
	// a comment
	m_ConstantColor = [ 255, 214, 200, 255 ]
	m_Renderers =
	[
		{
			_class = "C_OP_RenderDeferredLight"
			m_hTexture = resource:"materials/particle/x.vtex"
			m_flRange = -1.5e2
		},
	]
	"quoted key" = ""
	blob = #[ 00 FF 10 ]
	m_empty = {}
}"#;

    #[test]
    fn text_parses() {
        let v = parse_text(TEXT).unwrap();
        assert_eq!(v.at("m_nMaxParticles").and_then(Value::as_f64), Some(16.0));
        assert_eq!(v.at("m_Renderers.0.m_hTexture").and_then(Value::as_str), Some("materials/particle/x.vtex"));
        assert_eq!(v.at("m_Renderers.0.m_flRange").and_then(Value::as_f64), Some(-150.0));
        assert_eq!(v.get("m_ConstantColor").unwrap().numbers(), vec![255.0, 214.0, 200.0, 255.0]);
        assert_eq!(v.get("blob"), Some(&Value::Blob(vec![0, 255, 16])));
        assert_eq!(v.get("quoted key"), Some(&Value::Str(String::new())));
    }

    #[test]
    fn binary_round_trip() {
        let v = parse_text(TEXT).unwrap();
        let b = encode(&v);
        assert!(is_kv3(&b));
        assert_eq!(decode(&b).unwrap(), v);
    }

    /// The bytes Valve's files have for `{ k = #[01 02 03] }`, assembled field by field (the layout `tools/scene_lump.py`
    /// writes and Source 2 Viewer 21.1 reads): blobs in blocks, their sizes and a marker after the type bytes, the blob
    /// data after the two buffers, and a second marker after the data.
    fn golden_blob_doc() -> Vec<u8> {
        let u32le = |x: u32| x.to_le_bytes().to_vec();
        let marker = u32le(0xFFEE_DD00);
        let b0 = [b"k\0\0\0".to_vec(), u32le(1)].concat(); // "k", padded to 4, then the string count
        let b1 = [u32le(1), u32le(0), vec![9, 7], u32le(3), marker.clone()].concat(); // 1 member | key id 0 | types | blob size | marker
        let mut o = b"\x053VK".to_vec();
        o.extend_from_slice(&[0x7c, 0x16, 0x12, 0x74, 0xe9, 0x06, 0x98, 0x46, 0xaf, 0xf2, 0xe6, 0x3e, 0xb5, 0x90, 0x37, 0xe7]);
        o.extend([u32le(0), vec![0; 4]].concat()); // uncompressed, dictionary 0, frame 0
        o.extend([u32le(2), u32le(1), u32le(0), u32le(2)].concat()); // buffer 0: 2 bytes, 1 int; 2 type bytes
        o.extend([1u16.to_le_bytes(), 0u16.to_le_bytes()].concat()); // 1 object, 0 arrays
        o.extend([u32le(26), u32le(26), u32le(1), u32le(3), u32le(0), u32le(0)].concat()); // sizes; 1 block of 3 bytes
        o.extend([u32le(8), u32le(8), u32le(18), u32le(18)].concat()); // buffers 0 and 1
        o.extend([u32le(0), u32le(0), u32le(1), u32le(0)].concat()); // buffer 1: 0 bytes, 0 shorts, 1 int, 0 doubles
        o.extend([u32le(1), u32le(1), u32le(0), u32le(0)].concat()); // members, objects, arrays, spare
        o.extend([b0, b1, vec![1, 2, 3], marker].concat());
        o
    }

    #[test]
    fn blobs_live_in_blocks_in_the_layout_the_game_uses() {
        let v = Value::Object(vec![("k".into(), Value::Blob(vec![1, 2, 3]))]);
        let golden = golden_blob_doc();
        assert_eq!(encode(&v), golden, "the encoder writes what the game's files hold");
        assert_eq!(decode(&golden).unwrap(), v, "and the decoder reads the game's layout");
        // the header says one block of three bytes; the data sits after both buffers and the stream ends on a marker
        let b = encode(&v);
        assert_eq!(u32::from_le_bytes(b[56..60].try_into().unwrap()), 1, "block count");
        assert_eq!(u32::from_le_bytes(b[60..64].try_into().unwrap()), 3, "block bytes");
        assert!(b.ends_with(&[1, 2, 3, 0x00, 0xDD, 0xEE, 0xFF]), "blob data then the second 0xFFEEDD00");
    }

    #[test]
    fn several_blobs_empty_ones_and_other_values_survive() {
        // An entity lump's shape: objects with empty blobs among scalars, strings and arrays.
        let ent = |n: i64, blob: Vec<u8>| {
            Value::Object(vec![
                ("hammerUniqueId".into(), Value::Str(n.to_string())),
                ("origin".into(), Value::Array(vec![Value::Double(1.5), Value::Double(0.0), Value::Int(-7)])),
                ("m_keyValuesData".into(), Value::Blob(blob)),
            ])
        };
        let v = Value::Object(vec![
            ("m_entityKeyValues".into(), Value::Array(vec![ent(4, vec![]), ent(5, (0..=255).collect()), ent(6, vec![9]), ent(7, vec![])])),
            ("m_flag".into(), Value::Bool(true)),
        ]);
        let b = encode(&v);
        assert_eq!(decode(&b).unwrap(), v);
        // four blobs, 257 bytes, and the stream ends on the marker
        assert_eq!(u32::from_le_bytes(b[56..60].try_into().unwrap()), 4);
        assert_eq!(u32::from_le_bytes(b[60..64].try_into().unwrap()), 257);
        assert!(b.ends_with(&0xFFEE_DD00u32.to_le_bytes()));
        // a document without blobs is unchanged: no blocks, one marker, nothing after the buffers
        let plain = encode(&Value::Object(vec![("a".into(), Value::Int(5))]));
        assert_eq!(u32::from_le_bytes(plain[56..60].try_into().unwrap()), 0);
        assert!(plain.ends_with(&0xFFEE_DD00u32.to_le_bytes()));
        assert_eq!(decode(&plain).unwrap(), Value::Object(vec![("a".into(), Value::Int(5))]));
    }

    #[test]
    fn text_round_trip() {
        let v = parse_text(TEXT).unwrap();
        assert_eq!(parse_text(&v.to_text()).unwrap(), v);
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode(b"\x053VKshort").is_err());
        assert!(decode(b"nope").is_err());
    }
}
