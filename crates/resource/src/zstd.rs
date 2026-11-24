//! Zstandard decompression (RFC 8878), written here like the LZ4 decoder: no external crates.
//!
//! Decoder only: frames (and skippable frames), raw / RLE / compressed blocks, Huffman-coded literals (1 or 4
//! streams, tree reuse), FSE-coded sequences (predefined, RLE, compressed and repeated tables), repeat offsets,
//! and the frame checksum (XXH64). Dictionaries are refused with a clear error: Valve's KV3 blocks don't use one
//! (the `dict` field of every 6864 block read so far is 0).
//!
//! Where zstd shows up in Deadlock (build 6864): KV3 blocks with compression method 2, e.g. the map entity lumps
//! (`maps/<map>/entities/*.vents_c`) and some model and vdata blocks.

/// Decompress every frame in `src` (concatenated frames are joined, skippable frames skipped).
pub fn decompress(src: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut p = 0usize;
    while p < src.len() {
        p += frame(&src[p..], &mut out)?;
    }
    Ok(out)
}

/// Decompress and check the size.
pub fn decompress_exact(src: &[u8], expected: usize) -> Result<Vec<u8>, String> {
    let out = decompress(src)?;
    if out.len() != expected {
        return Err(format!("zstd: decoded {} bytes, expected {expected}", out.len()));
    }
    Ok(out)
}

const MAGIC: u32 = 0xFD2F_B528;
/// The largest block the format allows (RFC 8878, Block_Maximum_Size): it bounds what one RLE block can expand to.
const BLOCK_MAX: usize = 128 << 10;

fn le(b: &[u8], n: usize) -> Result<u64, String> {
    let s = b.get(..n).ok_or("zstd: truncated")?;
    Ok(s.iter().rev().fold(0u64, |a, &x| (a << 8) | x as u64))
}

/// One frame appended to `out`; returns the bytes it took.
fn frame(src: &[u8], out: &mut Vec<u8>) -> Result<usize, String> {
    let magic = le(src, 4)? as u32;
    if magic & 0xFFFF_FFF0 == 0x184D_2A50 {
        let n = le(&src[4..], 4)? as usize;
        if src.len() < 8 + n {
            return Err("zstd: truncated skippable frame".into());
        }
        return Ok(8 + n);
    }
    if magic != MAGIC {
        return Err(format!("zstd: bad magic {magic:08x}"));
    }
    let mut p = 4;
    let fhd = *src.get(p).ok_or("zstd: truncated header")?;
    p += 1;
    let fcs_flag = fhd >> 6;
    let single = fhd & 0x20 != 0;
    if fhd & 0x08 != 0 {
        return Err("zstd: reserved header bit set".into());
    }
    let checksum = fhd & 0x04 != 0;
    let dict_flag = fhd & 3;
    if !single {
        p += 1; // window descriptor: the whole frame is kept in `out`, so the window size is not needed
    }
    let dict_len = [0usize, 1, 2, 4][dict_flag as usize];
    if dict_len > 0 {
        let id = le(src.get(p..).ok_or("zstd: truncated header")?, dict_len)?;
        if id != 0 {
            return Err(format!("zstd: frame needs dictionary {id} (not supported)"));
        }
        p += dict_len;
    }
    let fcs_len = match fcs_flag {
        0 => usize::from(single),
        1 => 2,
        2 => 4,
        _ => 8,
    };
    let content_size = if fcs_len > 0 {
        let v = le(src.get(p..).ok_or("zstd: truncated header")?, fcs_len)?;
        p += fcs_len;
        Some(if fcs_len == 2 { v + 256 } else { v })
    } else {
        None
    };
    let start = out.len();
    if let Some(n) = content_size {
        // the header's size is a hint from the file: reserve only what a frame this long could plausibly hold
        out.reserve(n.min((src.len() as u64).saturating_mul(8).saturating_add(1 << 20)) as usize);
    }
    let mut st = State::new();
    loop {
        let h = le(src.get(p..).ok_or("zstd: truncated")?, 3)? as u32;
        p += 3;
        let last = h & 1 != 0;
        let kind = (h >> 1) & 3;
        let size = (h >> 3) as usize;
        if size > BLOCK_MAX {
            return Err(format!("zstd: block of {size} bytes, over the format's {BLOCK_MAX}"));
        }
        match kind {
            0 => {
                out.extend_from_slice(src.get(p..p + size).ok_or("zstd: truncated raw block")?);
                p += size;
            }
            1 => {
                let b = *src.get(p).ok_or("zstd: truncated RLE block")?;
                out.resize(out.len() + size, b);
                p += 1;
            }
            2 => {
                let blk = src.get(p..p + size).ok_or("zstd: truncated compressed block")?;
                st.block(blk, out, start)?;
                p += size;
            }
            _ => return Err("zstd: reserved block type".into()),
        }
        if last {
            break;
        }
    }
    if let Some(n) = content_size {
        if (out.len() - start) as u64 != n {
            return Err(format!("zstd: frame decoded {} bytes, header says {n}", out.len() - start));
        }
    }
    if checksum {
        let want = le(src.get(p..).ok_or("zstd: truncated checksum")?, 4)? as u32;
        let got = xxh64(&out[start..], 0) as u32;
        if want != got {
            return Err(format!("zstd: checksum mismatch ({got:08x}, frame says {want:08x})"));
        }
        p += 4;
    }
    Ok(p)
}

// ---- bit readers ---------------------------------------------------------------------------------------------

/// Little-endian bits read forwards (FSE table descriptions).
struct Fwd<'a> {
    b: &'a [u8],
    bit: usize,
}
impl<'a> Fwd<'a> {
    fn get(&mut self, n: u32) -> u32 {
        let v = self.peek(n);
        self.bit += n as usize;
        v
    }
    fn peek(&self, n: u32) -> u32 {
        let mut v = 0u64;
        let byte = self.bit / 8;
        for i in 0..8 {
            if let Some(&x) = self.b.get(byte + i) {
                v |= (x as u64) << (8 * i);
            }
        }
        ((v >> (self.bit % 8)) & ((1u64 << n) - 1)) as u32
    }
    fn bytes_used(&self) -> usize {
        self.bit.div_ceil(8)
    }
}

/// Bits read backwards from the end of a stream (Huffman and FSE streams). The last byte holds a 1 bit marking
/// where the data ends; reads past the start give zeros, and `left()` goes negative.
struct Back<'a> {
    b: &'a [u8],
    pos: i64, // bits still unread, counted from the start of the stream
}
impl<'a> Back<'a> {
    fn new(b: &'a [u8]) -> Result<Self, String> {
        let last = *b.last().ok_or("zstd: empty bitstream")?;
        if last == 0 {
            return Err("zstd: bitstream without its end marker".into());
        }
        let hb = 7 - last.leading_zeros() as i64;
        Ok(Back { b, pos: (b.len() as i64 - 1) * 8 + hb })
    }
    fn bits_at(&self, start: i64, n: u32) -> u64 {
        if n == 0 {
            return 0;
        }
        if start < 0 {
            let have = n as i64 + start;
            return if have <= 0 { 0 } else { self.bits_at(0, have as u32) << (-start) };
        }
        let byte = (start / 8) as usize;
        let mut v = 0u64;
        for i in 0..8 {
            if let Some(&x) = self.b.get(byte + i) {
                v |= (x as u64) << (8 * i);
            }
        }
        (v >> (start % 8)) & ((1u64 << n) - 1)
    }
    fn peek(&self, n: u32) -> u64 {
        self.bits_at(self.pos - n as i64, n)
    }
    fn get(&mut self, n: u32) -> u64 {
        let v = self.peek(n);
        self.pos -= n as i64;
        v
    }
    fn left(&self) -> i64 {
        self.pos
    }
}

// ---- FSE --------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct FseCell {
    sym: u8,
    bits: u8,
    base: u16,
}

#[derive(Clone, Default)]
struct Fse {
    log: u32,
    cells: Vec<FseCell>,
}

impl Fse {
    fn from_counts(counts: &[i32], log: u32) -> Result<Fse, String> {
        let size = 1usize << log;
        let mut cells = vec![FseCell::default(); size];
        let mut high = size - 1;
        let mut next = vec![0u32; counts.len()];
        for (s, &c) in counts.iter().enumerate() {
            if c == -1 {
                cells[high].sym = s as u8;
                high = high.wrapping_sub(1);
                next[s] = 1;
            } else {
                next[s] = c.max(0) as u32;
            }
        }
        let step = (size >> 1) + (size >> 3) + 3;
        let mask = size - 1;
        let mut pos = 0usize;
        for (s, &c) in counts.iter().enumerate() {
            for _ in 0..c.max(0) {
                cells[pos].sym = s as u8;
                loop {
                    pos = (pos + step) & mask;
                    if pos <= high || high == usize::MAX {
                        break;
                    }
                }
            }
        }
        if pos != 0 {
            return Err("zstd: FSE distribution does not fill its table".into());
        }
        for cell in cells.iter_mut() {
            let s = cell.sym as usize;
            let ns = next[s];
            next[s] += 1;
            let nb = log - (31 - ns.leading_zeros());
            cell.bits = nb as u8;
            cell.base = ((ns << nb) as usize - size) as u16;
        }
        Ok(Fse { log, cells })
    }

    /// A table description (RFC 8878 4.1.1); returns the table and the bytes it used.
    fn read(src: &[u8], max_log: u32, max_sym: usize) -> Result<(Fse, usize), String> {
        let mut r = Fwd { b: src, bit: 0 };
        let log = r.get(4) + 5;
        if log > max_log {
            return Err(format!("zstd: FSE accuracy {log} above {max_log}"));
        }
        let mut remaining: i32 = (1 << log) + 1;
        let mut threshold: i32 = 1 << log;
        let mut nb = log + 1;
        let mut counts: Vec<i32> = Vec::new();
        while remaining > 1 {
            if counts.len() > max_sym {
                return Err("zstd: FSE description has too many symbols".into());
            }
            let max = (2 * threshold - 1) - remaining;
            let low = r.peek(nb - 1) as i32;
            let mut v;
            if low < max {
                v = low;
                r.bit += (nb - 1) as usize;
            } else {
                v = r.peek(nb) as i32;
                if v >= threshold {
                    v -= max;
                }
                r.bit += nb as usize;
            }
            let c = v - 1;
            remaining -= c.abs();
            counts.push(c);
            if c == 0 {
                loop {
                    let rep = r.get(2);
                    counts.extend(std::iter::repeat_n(0, rep as usize));
                    if rep != 3 {
                        break;
                    }
                }
            }
            while remaining < threshold && nb > 1 {
                nb -= 1;
                threshold >>= 1;
            }
        }
        if remaining != 1 || counts.len() > max_sym + 1 {
            return Err("zstd: bad FSE description".into());
        }
        Ok((Fse::from_counts(&counts, log)?, r.bytes_used()))
    }

    fn rle(sym: u8) -> Fse {
        Fse { log: 0, cells: vec![FseCell { sym, bits: 0, base: 0 }] }
    }
}

struct FseState<'t> {
    t: &'t Fse,
    s: usize,
}
impl<'t> FseState<'t> {
    fn new(t: &'t Fse, br: &mut Back) -> Self {
        FseState { t, s: br.get(t.log) as usize }
    }
    fn sym(&self) -> u8 {
        self.t.cells[self.s].sym
    }
    fn update(&mut self, br: &mut Back) {
        let c = self.t.cells[self.s];
        self.s = c.base as usize + br.get(c.bits as u32) as usize;
    }
}

// ---- Huffman ----------------------------------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Huff {
    max_bits: u32,
    table: Vec<(u8, u8)>, // (symbol, bits) indexed by the next max_bits bits
}

impl Huff {
    /// A tree description; returns the table and the bytes it used.
    fn read(src: &[u8]) -> Result<(Huff, usize), String> {
        let h = *src.first().ok_or("zstd: missing Huffman tree")? as usize;
        let mut w: Vec<u8> = Vec::new();
        let used;
        if h < 128 {
            let body = src.get(1..1 + h).ok_or("zstd: truncated Huffman weights")?;
            let (t, n) = Fse::read(body, 6, 255)?;
            let mut br = Back::new(body.get(n..).ok_or("zstd: Huffman weight table past its description")?)?;
            let mut s1 = FseState::new(&t, &mut br);
            let mut s2 = FseState::new(&t, &mut br);
            loop {
                w.push(s1.sym());
                s1.update(&mut br);
                if br.left() < 0 {
                    w.push(s2.sym());
                    break;
                }
                w.push(s2.sym());
                s2.update(&mut br);
                if br.left() < 0 {
                    w.push(s1.sym());
                    break;
                }
                if w.len() > 255 {
                    return Err("zstd: too many Huffman weights".into());
                }
            }
            used = 1 + h;
        } else {
            let n = h - 127;
            let body = src.get(1..1 + n.div_ceil(2)).ok_or("zstd: truncated Huffman weights")?;
            for i in 0..n {
                let b = body[i / 2];
                w.push(if i % 2 == 0 { b >> 4 } else { b & 15 });
            }
            used = 1 + n.div_ceil(2);
        }
        // a weight is a code length in disguise (at most 12, RFC 8878 4.2.1); an FSE-coded table can say anything
        if let Some(&x) = w.iter().find(|&&x| x > 12) {
            return Err(format!("zstd: Huffman weight {x}, over 12"));
        }
        let sum: u32 = w.iter().filter(|&&x| x > 0).map(|&x| 1u32 << (x - 1)).sum();
        if sum == 0 {
            return Err("zstd: empty Huffman tree".into());
        }
        let max_bits = 32 - sum.leading_zeros(); // the total rounds up to the next power of two
        let total = 1u32 << max_bits;
        let rest = total - sum;
        if !rest.is_power_of_two() {
            return Err("zstd: Huffman weights don't complete a tree".into());
        }
        w.push((rest.trailing_zeros() + 1) as u8);
        if max_bits > 11 {
            return Err(format!("zstd: Huffman code of {max_bits} bits"));
        }
        let mut count = [0u32; 13];
        for &x in &w {
            count[x as usize] += 1;
        }
        let mut start = [0u32; 13];
        let mut next = 0u32;
        for x in 1..=max_bits as usize {
            start[x] = next;
            next += count[x] << (x - 1);
        }
        let mut table = vec![(0u8, 0u8); 1 << max_bits];
        for (sym, &x) in w.iter().enumerate() {
            if x == 0 {
                continue;
            }
            let len = 1u32 << (x - 1);
            let bits = (max_bits + 1 - x as u32) as u8;
            for i in start[x as usize]..start[x as usize] + len {
                table[i as usize] = (sym as u8, bits);
            }
            start[x as usize] += len;
        }
        Ok((Huff { max_bits, table }, used))
    }

    fn stream(&self, src: &[u8], n: usize, out: &mut Vec<u8>) -> Result<(), String> {
        let mut br = Back::new(src)?;
        for _ in 0..n {
            let (s, b) = self.table[br.peek(self.max_bits) as usize];
            out.push(s);
            br.pos -= b as i64;
        }
        if br.left() != 0 {
            return Err("zstd: Huffman stream not consumed exactly".into());
        }
        Ok(())
    }
}

// ---- blocks -----------------------------------------------------------------------------------------------------

const LL_BASE: [(u32, u32); 36] = [
    (0, 0), (1, 0), (2, 0), (3, 0), (4, 0), (5, 0), (6, 0), (7, 0), (8, 0), (9, 0), (10, 0), (11, 0), (12, 0), (13, 0),
    (14, 0), (15, 0), (16, 1), (18, 1), (20, 1), (22, 1), (24, 2), (28, 2), (32, 3), (40, 3), (48, 4), (64, 6),
    (128, 7), (256, 8), (512, 9), (1024, 10), (2048, 11), (4096, 12), (8192, 13), (16384, 14), (32768, 15), (65536, 16),
];
const ML_BASE: [(u32, u32); 53] = [
    (3, 0), (4, 0), (5, 0), (6, 0), (7, 0), (8, 0), (9, 0), (10, 0), (11, 0), (12, 0), (13, 0), (14, 0), (15, 0),
    (16, 0), (17, 0), (18, 0), (19, 0), (20, 0), (21, 0), (22, 0), (23, 0), (24, 0), (25, 0), (26, 0), (27, 0),
    (28, 0), (29, 0), (30, 0), (31, 0), (32, 0), (33, 0), (34, 0), (35, 1), (37, 1), (39, 1), (41, 1), (43, 2),
    (47, 2), (51, 3), (59, 3), (67, 4), (83, 4), (99, 5), (131, 7), (259, 8), (515, 9), (1027, 10), (2051, 11),
    (4099, 12), (8195, 13), (16387, 14), (32771, 15), (65539, 16),
];
const LL_DEFAULT: [i32; 36] = [4, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 2, 1, 1, 1, 1, 1, -1, -1, -1, -1];
const ML_DEFAULT: [i32; 53] = [
    1, 4, 3, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, -1, -1, -1, -1, -1, -1, -1,
];
const OF_DEFAULT: [i32; 29] = [1, 1, 1, 1, 1, 1, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, -1, -1, -1, -1, -1];

/// What carries over from block to block within a frame.
struct State {
    huff: Option<Huff>,
    ll: Option<Fse>,
    of: Option<Fse>,
    ml: Option<Fse>,
    rep: [usize; 3],
}

impl State {
    fn new() -> Self {
        State { huff: None, ll: None, of: None, ml: None, rep: [1, 4, 8] }
    }

    fn block(&mut self, b: &[u8], out: &mut Vec<u8>, frame_start: usize) -> Result<(), String> {
        let (lits, mut p) = self.literals(b)?;
        // sequences
        let b0 = *b.get(p).ok_or("zstd: truncated sequences header")? as usize;
        let nseq = if b0 < 128 {
            p += 1;
            b0
        } else if b0 < 255 {
            let v = ((b0 - 128) << 8) + *b.get(p + 1).ok_or("zstd: truncated")? as usize;
            p += 2;
            v
        } else {
            let v = le(&b[p + 1..], 2)? as usize + 0x7F00;
            p += 3;
            v
        };
        if nseq == 0 {
            out.extend_from_slice(&lits);
            return Ok(());
        }
        let modes = *b.get(p).ok_or("zstd: truncated sequence modes")?;
        p += 1;
        if modes & 3 != 0 {
            return Err("zstd: reserved bits in sequence modes".into());
        }
        let ll = Self::table(&mut self.ll, modes >> 6, b, &mut p, &LL_DEFAULT, 6, 9, 35)?;
        let of = Self::table(&mut self.of, (modes >> 4) & 3, b, &mut p, &OF_DEFAULT, 5, 8, 31)?;
        let ml = Self::table(&mut self.ml, (modes >> 2) & 3, b, &mut p, &ML_DEFAULT, 6, 9, 52)?;
        let (ll, of, ml) = (ll.clone(), of.clone(), ml.clone());
        let mut br = Back::new(b.get(p..).ok_or("zstd: missing sequence bitstream")?)?;
        let mut sl = FseState::new(&ll, &mut br);
        let mut so = FseState::new(&of, &mut br);
        let mut sm = FseState::new(&ml, &mut br);
        let mut lp = 0usize;
        for i in 0..nseq {
            let oc = so.sym() as u32;
            if oc > 31 {
                return Err("zstd: offset code above 31".into());
            }
            let (mb, mx) = *ML_BASE.get(sm.sym() as usize).ok_or("zstd: bad match length code")?;
            let (lb, lx) = *LL_BASE.get(sl.sym() as usize).ok_or("zstd: bad literal length code")?;
            let ofv = (1u64 << oc) + br.get(oc);
            let mlen = (mb as u64 + br.get(mx)) as usize;
            let llen = (lb as u64 + br.get(lx)) as usize;
            // repeat offsets (RFC 8878 3.1.1.5)
            let off = if ofv > 3 {
                let o = (ofv - 3) as usize;
                self.rep = [o, self.rep[0], self.rep[1]];
                o
            } else {
                let idx = ofv as usize + usize::from(llen == 0);
                match idx {
                    1 => self.rep[0],
                    2 => {
                        let o = self.rep[1];
                        self.rep = [o, self.rep[0], self.rep[2]];
                        o
                    }
                    3 => {
                        let o = self.rep[2];
                        self.rep = [o, self.rep[0], self.rep[1]];
                        o
                    }
                    _ => {
                        let o = self.rep[0].checked_sub(1).filter(|&o| o > 0).ok_or("zstd: repeat offset 0")?;
                        self.rep = [o, self.rep[0], self.rep[1]];
                        o
                    }
                }
            };
            let l = lits.get(lp..lp + llen).ok_or("zstd: sequence runs past the literals")?;
            out.extend_from_slice(l);
            lp += llen;
            if off > out.len() - frame_start {
                return Err(format!("zstd: offset {off} reaches before the frame"));
            }
            let from = out.len() - off;
            if off >= mlen {
                out.extend_from_within(from..from + mlen);
            } else {
                for k in 0..mlen {
                    let c = out[from + k];
                    out.push(c);
                }
            }
            if i + 1 < nseq {
                sl.update(&mut br);
                sm.update(&mut br);
                so.update(&mut br);
            }
        }
        if br.left() != 0 {
            return Err("zstd: sequence bitstream not consumed exactly".into());
        }
        out.extend_from_slice(&lits[lp..]);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn table<'s>(slot: &'s mut Option<Fse>, mode: u8, b: &[u8], p: &mut usize, def: &[i32], def_log: u32, max_log: u32,
                 max_sym: usize) -> Result<&'s Fse, String> {
        match mode {
            0 => *slot = Some(Fse::from_counts(def, def_log)?),
            1 => {
                let s = *b.get(*p).ok_or("zstd: truncated RLE table")?;
                *p += 1;
                *slot = Some(Fse::rle(s));
            }
            2 => {
                let (t, n) = Fse::read(b.get(*p..).ok_or("zstd: truncated FSE table")?, max_log, max_sym)?;
                *p += n;
                *slot = Some(t);
            }
            _ => {
                if slot.is_none() {
                    return Err("zstd: repeat table with no table before it".into());
                }
            }
        }
        slot.as_ref().ok_or_else(|| "zstd: no table".to_string())
    }

    fn literals(&mut self, b: &[u8]) -> Result<(Vec<u8>, usize), String> {
        let b0 = *b.first().ok_or("zstd: empty block")? as usize;
        let kind = b0 & 3;
        let sf = (b0 >> 2) & 3;
        if kind < 2 {
            let (regen, hl) = match sf {
                0 | 2 => (b0 >> 3, 1),
                1 => ((b0 >> 4) + ((*b.get(1).ok_or("zstd: truncated")? as usize) << 4), 2),
                _ => ((b0 >> 4) + ((le(&b[1..], 2)? as usize) << 4), 3),
            };
            if kind == 0 {
                let l = b.get(hl..hl + regen).ok_or("zstd: truncated raw literals")?.to_vec();
                return Ok((l, hl + regen));
            }
            let c = *b.get(hl).ok_or("zstd: truncated RLE literals")?;
            return Ok((vec![c; regen], hl + 1));
        }
        let (regen, comp, hl, streams) = match sf {
            0 | 1 => {
                let h = le(b, 3)? as usize;
                ((h >> 4) & 0x3FF, (h >> 14) & 0x3FF, 3, if sf == 0 { 1 } else { 4 })
            }
            2 => {
                let h = le(b, 4)? as usize;
                ((h >> 4) & 0x3FFF, (h >> 18) & 0x3FFF, 4, 4)
            }
            _ => {
                let h = le(b, 5)? as usize;
                ((h >> 4) & 0x3FFFF, (h >> 22) & 0x3FFFF, 5, 4)
            }
        };
        let body = b.get(hl..hl + comp).ok_or("zstd: truncated compressed literals")?;
        let mut q = 0;
        if kind == 2 {
            let (h, n) = Huff::read(body)?;
            self.huff = Some(h);
            q = n;
        }
        let huff = self.huff.as_ref().ok_or("zstd: treeless literals with no tree before them")?;
        let data = &body[q..];
        let mut out = Vec::with_capacity(regen);
        if streams == 1 {
            huff.stream(data, regen, &mut out)?;
        } else {
            let s1 = le(data, 2)? as usize;
            let s2 = le(&data[2..], 2)? as usize;
            let s3 = le(&data[4..], 2)? as usize;
            let rest = data.len().checked_sub(6 + s1 + s2 + s3).ok_or("zstd: bad literal jump table")?;
            let per = regen.div_ceil(4);
            let mut at = 6;
            for (k, len) in [s1, s2, s3, rest].into_iter().enumerate() {
                let n = if k < 3 { per } else { regen.checked_sub(3 * per).ok_or("zstd: bad literal sizes")? };
                huff.stream(&data[at..at + len], n, &mut out)?;
                at += len;
            }
        }
        Ok((out, hl + comp))
    }
}

// ---- XXH64 ------------------------------------------------------------------------------------------------------

const P1: u64 = 0x9E37_79B1_85EB_CA87;
const P2: u64 = 0xC2B2_AE3D_27D4_EB4F;
const P3: u64 = 0x1656_67B1_9E37_79F9;
const P4: u64 = 0x85EB_CA77_C2B2_AE63;
const P5: u64 = 0x27D4_EB2F_1656_67C5;

fn round(acc: u64, v: u64) -> u64 {
    acc.wrapping_add(v.wrapping_mul(P2)).rotate_left(31).wrapping_mul(P1)
}
fn merge(acc: u64, v: u64) -> u64 {
    (acc ^ round(0, v)).wrapping_mul(P1).wrapping_add(P4)
}

/// XXH64 (the zstd frame checksum is its low 32 bits).
pub fn xxh64(d: &[u8], seed: u64) -> u64 {
    let u64at = |i: usize| u64::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3], d[i + 4], d[i + 5], d[i + 6], d[i + 7]]);
    let u32at = |i: usize| u32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]) as u64;
    let mut i = 0;
    let mut h = if d.len() >= 32 {
        let mut v = [seed.wrapping_add(P1).wrapping_add(P2), seed.wrapping_add(P2), seed, seed.wrapping_sub(P1)];
        while i + 32 <= d.len() {
            for (k, vk) in v.iter_mut().enumerate() {
                *vk = round(*vk, u64at(i + 8 * k));
            }
            i += 32;
        }
        let mut h = v[0].rotate_left(1).wrapping_add(v[1].rotate_left(7)).wrapping_add(v[2].rotate_left(12)).wrapping_add(v[3].rotate_left(18));
        for vk in v {
            h = merge(h, vk);
        }
        h
    } else {
        seed.wrapping_add(P5)
    };
    h = h.wrapping_add(d.len() as u64);
    while i + 8 <= d.len() {
        h ^= round(0, u64at(i));
        h = h.rotate_left(27).wrapping_mul(P1).wrapping_add(P4);
        i += 8;
    }
    if i + 4 <= d.len() {
        h ^= u32at(i).wrapping_mul(P1);
        h = h.rotate_left(23).wrapping_mul(P2).wrapping_add(P3);
        i += 4;
    }
    while i < d.len() {
        h ^= (d[i] as u64).wrapping_mul(P5);
        h = h.rotate_left(11).wrapping_mul(P1);
        i += 1;
    }
    h ^= h >> 33;
    h = h.wrapping_mul(P2);
    h ^= h >> 29;
    h = h.wrapping_mul(P3);
    h ^ (h >> 32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xxh64_known_values() {
        // reference values of the XXH64 specification's implementation
        assert_eq!(xxh64(b"", 0), 0xEF46_DB37_51D8_E999);
        assert_eq!(xxh64(b"a", 0), 0xD24E_C4F1_A98C_6E5B);
        assert_eq!(xxh64(b"abc", 0), 0x44BC_2CF5_AD77_0999);
        assert_eq!(xxh64(b"Nobody inspects the spammish repetition", 0), 0xFBCE_A83C_8A37_8BF1);
    }

    fn frame_of(blocks: &[(bool, u32, &[u8], u32)]) -> Vec<u8> {
        // (last, type, payload, size field) blocks in a single-segment-less frame with no content size
        let mut f = vec![0x28, 0xB5, 0x2F, 0xFD, 0x00, 0x00];
        for &(last, ty, payload, size) in blocks {
            let h = u32::from(last) | (ty << 1) | (size << 3);
            f.extend_from_slice(&h.to_le_bytes()[..3]);
            f.extend_from_slice(payload);
        }
        f
    }

    #[test]
    fn raw_and_rle_blocks() {
        let f = frame_of(&[(false, 0, b"hello ", 6), (true, 1, b"z", 4)]);
        assert_eq!(decompress(&f).unwrap(), b"hello zzzz");
    }

    #[test]
    fn compressed_block_raw_literals_no_sequences() {
        // literals: raw, 5 bytes (size format 0: b0 = 5 << 3); sequences: 0
        let payload = [5u8 << 3, b'w', b'o', b'r', b'l', b'd', 0];
        let f = frame_of(&[(true, 2, &payload, payload.len() as u32)]);
        assert_eq!(decompress(&f).unwrap(), b"world");
    }

    #[test]
    fn skippable_frames_and_concatenation() {
        let mut f = vec![0x50, 0x2A, 0x4D, 0x18, 3, 0, 0, 0, 1, 2, 3];
        f.extend(frame_of(&[(true, 0, b"ab", 2)]));
        f.extend(frame_of(&[(true, 0, b"cd", 2)]));
        assert_eq!(decompress(&f).unwrap(), b"abcd");
    }

    #[test]
    fn refuses_garbage() {
        assert!(decompress(b"not zstd").is_err());
        let f = frame_of(&[(true, 3, b"", 0)]);
        assert!(decompress(&f).unwrap_err().contains("reserved"));
    }

    #[test]
    fn predefined_tables_build() {
        assert_eq!(Fse::from_counts(&LL_DEFAULT, 6).unwrap().cells.len(), 64);
        assert_eq!(Fse::from_counts(&ML_DEFAULT, 6).unwrap().cells.len(), 64);
        assert_eq!(Fse::from_counts(&OF_DEFAULT, 5).unwrap().cells.len(), 32);
    }

    /// Frames from libzstd 1.5.7 (Windows' tar.exe, `--zstd`, levels 1 and 19) over synthetic text, beside the
    /// expected bytes: Huffman literals (1 and 4 streams), compressed and repeated FSE tables, repeat offsets,
    /// checksums. Generated by `tests/zstd/make_vectors.py`; no game data.
    #[test]
    fn libzstd_vectors() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("zstd");
        let mut n = 0;
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.extension().and_then(|x| x.to_str()) != Some("zst") {
                continue;
            }
            let want = std::fs::read(p.with_extension("out")).unwrap();
            let got = decompress(&std::fs::read(&p).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            assert!(got == want, "{}: {} bytes, expected {}", p.display(), got.len(), want.len());
            n += 1;
        }
        assert!(n >= 4, "expected the libzstd vectors in {}", dir.display());
    }
}
