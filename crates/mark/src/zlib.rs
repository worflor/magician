//! zlib (RFC 1950) over deflate (RFC 1951), both directions, no dependencies.
//!
//! Inflate handles stored, fixed and dynamic blocks with a 15-bit lookup table per Huffman code. Deflate is
//! LZ77 over a 32 KiB window (hash chains, one step of lazy matching) with dynamic Huffman codes per block,
//! length-limited to 15 bits by frequency halving. Deterministic: the same input always gives the same bytes.

// ---------------------------------------------------------------------------------------------- shared tables

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
const CL_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += x as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

// ---------------------------------------------------------------------------------------------------- inflate

struct BitIn<'a> {
    d: &'a [u8],
    pos: usize,
    buf: u64,
    n: u32,
}

impl<'a> BitIn<'a> {
    fn refill(&mut self) {
        while self.n <= 56 {
            let b = if self.pos < self.d.len() { self.d[self.pos] } else { 0 };
            self.pos += 1;
            self.buf |= (b as u64) << self.n;
            self.n += 8;
        }
    }
    fn bits(&mut self, k: u32) -> Result<u32, String> {
        if k == 0 {
            return Ok(0);
        }
        self.refill();
        if self.pos > self.d.len() + 8 {
            return Err("deflate stream truncated".into());
        }
        let v = (self.buf & ((1u64 << k) - 1)) as u32;
        self.buf >>= k;
        self.n -= k;
        Ok(v)
    }
    fn align(&mut self) {
        let r = self.n % 8;
        self.buf >>= r;
        self.n -= r;
    }
}

/// A canonical Huffman decoder: a table indexed by the next 15 input bits (LSB first).
struct Huff {
    table: Vec<u32>, // symbol << 4 | length; 0 = invalid
}

impl Huff {
    fn new(lengths: &[u8]) -> Result<Huff, String> {
        let mut count = [0u32; 16];
        for &l in lengths {
            count[l as usize] += 1;
        }
        count[0] = 0;
        let mut next = [0u32; 16];
        let mut code = 0u32;
        for bits in 1..16 {
            code = (code + count[bits - 1]) << 1;
            next[bits] = code;
        }
        let mut table = vec![0u32; 1 << 15];
        for (sym, &l) in lengths.iter().enumerate() {
            if l == 0 {
                continue;
            }
            let c = next[l as usize];
            next[l as usize] += 1;
            if c >= (1 << l) {
                return Err("over-subscribed Huffman code".into());
            }
            let rev = (c.reverse_bits() >> (32 - l as u32)) as usize;
            let mut i = rev;
            while i < (1 << 15) {
                table[i] = ((sym as u32) << 4) | l as u32;
                i += 1 << l;
            }
        }
        Ok(Huff { table })
    }
    fn decode(&self, r: &mut BitIn) -> Result<u32, String> {
        r.refill();
        // past the end the reader sees zero bits; a code made of them must not decode forever
        if r.pos > r.d.len() + 8 {
            return Err("deflate stream truncated".into());
        }
        let e = self.table[(r.buf & 0x7fff) as usize];
        if e == 0 {
            return Err("invalid Huffman code".into());
        }
        let l = e & 15;
        r.buf >>= l;
        r.n -= l;
        Ok(e >> 4)
    }
}

/// Inflate a raw deflate stream.
pub fn inflate(src: &[u8]) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = Vec::with_capacity(src.len() * 4);
    let mut r = BitIn { d: src, pos: 0, buf: 0, n: 0 };
    loop {
        let last = r.bits(1)?;
        let ty = r.bits(2)?;
        match ty {
            0 => {
                r.align();
                let len = r.bits(16)? as usize;
                let nlen = r.bits(16)? as usize;
                if len != (!nlen & 0xffff) {
                    return Err("stored block length check failed".into());
                }
                for _ in 0..len {
                    out.push(r.bits(8)? as u8);
                }
            }
            1 | 2 => {
                let (lit, dist) = if ty == 1 {
                    let mut l = [0u8; 288];
                    l[..144].fill(8);
                    l[144..256].fill(9);
                    l[256..280].fill(7);
                    l[280..].fill(8);
                    (Huff::new(&l)?, Huff::new(&[5u8; 30])?)
                } else {
                    let hlit = r.bits(5)? as usize + 257;
                    let hdist = r.bits(5)? as usize + 1;
                    let hclen = r.bits(4)? as usize + 4;
                    let mut cl = [0u8; 19];
                    for &i in CL_ORDER.iter().take(hclen) {
                        cl[i] = r.bits(3)? as u8;
                    }
                    let clh = Huff::new(&cl)?;
                    let mut lens = Vec::with_capacity(hlit + hdist);
                    while lens.len() < hlit + hdist {
                        let s = clh.decode(&mut r)?;
                        match s {
                            0..=15 => lens.push(s as u8),
                            16 => {
                                let prev = *lens.last().ok_or("repeat with no previous length")?;
                                for _ in 0..3 + r.bits(2)? {
                                    lens.push(prev);
                                }
                            }
                            17 => lens.extend(std::iter::repeat_n(0, 3 + r.bits(3)? as usize)),
                            _ => lens.extend(std::iter::repeat_n(0, 11 + r.bits(7)? as usize)),
                        }
                    }
                    if lens.len() > hlit + hdist {
                        return Err("code lengths overrun".into());
                    }
                    (Huff::new(&lens[..hlit])?, Huff::new(&lens[hlit..])?)
                };
                loop {
                    let s = lit.decode(&mut r)?;
                    if s < 256 {
                        out.push(s as u8);
                    } else if s == 256 {
                        break;
                    } else {
                        let i = (s - 257) as usize;
                        if i >= 29 {
                            return Err("bad length symbol".into());
                        }
                        let len = LEN_BASE[i] as usize + r.bits(LEN_EXTRA[i] as u32)? as usize;
                        let ds = dist.decode(&mut r)? as usize;
                        if ds >= 30 {
                            return Err("bad distance symbol".into());
                        }
                        let d = DIST_BASE[ds] as usize + r.bits(DIST_EXTRA[ds] as u32)? as usize;
                        if d > out.len() {
                            return Err("distance before start of output".into());
                        }
                        let start = out.len() - d;
                        for k in 0..len {
                            let b = out[start + k];
                            out.push(b);
                        }
                    }
                }
            }
            _ => return Err("reserved block type".into()),
        }
        if last == 1 {
            break;
        }
        if r.pos > src.len() + 8 {
            return Err("deflate stream truncated".into());
        }
    }
    Ok(out)
}

/// Inflate a zlib stream (header, deflate data, Adler-32 checked).
pub fn unzlib(src: &[u8]) -> Result<Vec<u8>, String> {
    if src.len() < 6 || (src[0] & 15) != 8 || !((src[0] as u16) << 8 | src[1] as u16).is_multiple_of(31) {
        return Err("not a zlib stream".into());
    }
    if src[1] & 0x20 != 0 {
        return Err("zlib preset dictionary not supported".into());
    }
    let out = inflate(&src[2..])?;
    let want = u32::from_be_bytes([src[src.len() - 4], src[src.len() - 3], src[src.len() - 2], src[src.len() - 1]]);
    if adler32(&out) != want {
        return Err("zlib Adler-32 mismatch".into());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------- deflate

struct BitOut {
    out: Vec<u8>,
    buf: u64,
    n: u32,
}

impl BitOut {
    fn put(&mut self, v: u32, k: u32) {
        self.buf |= (v as u64) << self.n;
        self.n += k;
        while self.n >= 8 {
            self.out.push(self.buf as u8);
            self.buf >>= 8;
            self.n -= 8;
        }
    }
    fn flush(&mut self) {
        if self.n > 0 {
            self.out.push(self.buf as u8);
        }
        self.buf = 0;
        self.n = 0;
    }
}

/// Code lengths of a Huffman code for `freq`, no longer than `limit` (frequencies are halved until it fits).
fn huff_lengths(freq: &[u32], limit: u8) -> Vec<u8> {
    let mut f: Vec<u64> = freq.iter().map(|&x| x as u64).collect();
    loop {
        let mut leaves: Vec<(u64, usize)> = f.iter().enumerate().filter(|(_, &w)| w > 0).map(|(i, &w)| (w, i)).collect();
        let mut lens = vec![0u8; f.len()];
        if leaves.is_empty() {
            return lens;
        }
        if leaves.len() == 1 {
            lens[leaves[0].1] = 1;
            return lens;
        }
        leaves.sort();
        // two-queue Huffman: nodes 0..n leaves, n.. internal
        let n = leaves.len();
        let mut weight: Vec<u64> = leaves.iter().map(|l| l.0).collect();
        let mut parent = vec![usize::MAX; 2 * n - 1];
        let (mut li, mut qi) = (0usize, n);
        let take = |weight: &Vec<u64>, li: &mut usize, qi: &mut usize, end: usize| -> usize {
            if *li < n && (*qi >= end || weight[*li] <= weight[*qi]) {
                *li += 1;
                *li - 1
            } else {
                *qi += 1;
                *qi - 1
            }
        };
        for k in n..2 * n - 1 {
            let a = take(&weight, &mut li, &mut qi, k);
            let b = take(&weight, &mut li, &mut qi, k);
            weight.push(weight[a] + weight[b]);
            parent[a] = k;
            parent[b] = k;
        }
        let mut depth = vec![0u8; 2 * n - 1];
        let mut max = 0;
        for k in (0..2 * n - 2).rev() {
            depth[k] = depth[parent[k]] + 1;
            max = max.max(depth[k]);
        }
        if max <= limit {
            for (k, l) in leaves.iter().enumerate() {
                lens[l.1] = depth[k];
            }
            return lens;
        }
        for w in f.iter_mut() {
            if *w > 0 {
                *w = (*w >> 1).max(1);
            }
        }
    }
}

/// Canonical codes for lengths, bit-reversed for LSB-first output.
fn codes(lens: &[u8]) -> Vec<u32> {
    let mut count = [0u32; 16];
    for &l in lens {
        count[l as usize] += 1;
    }
    count[0] = 0;
    let mut next = [0u32; 16];
    let mut code = 0;
    for b in 1..16 {
        code = (code + count[b - 1]) << 1;
        next[b] = code;
    }
    lens.iter()
        .map(|&l| {
            if l == 0 {
                0
            } else {
                let c = next[l as usize];
                next[l as usize] += 1;
                c.reverse_bits() >> (32 - l as u32)
            }
        })
        .collect()
}

#[derive(Clone, Copy)]
enum Sym {
    Lit(u8),
    Match(u16, u16), // length, distance
}

fn len_code(len: u16) -> (usize, u32, u32) {
    let mut i = 28;
    while LEN_BASE[i] > len {
        i -= 1;
    }
    (257 + i, (len - LEN_BASE[i]) as u32, LEN_EXTRA[i] as u32)
}

fn dist_code(d: u16) -> (usize, u32, u32) {
    let mut i = 29;
    while DIST_BASE[i] > d {
        i -= 1;
    }
    (i, (d - DIST_BASE[i]) as u32, DIST_EXTRA[i] as u32)
}

fn emit_block(w: &mut BitOut, syms: &[Sym], last: bool) {
    let mut lf = vec![0u32; 286];
    let mut df = vec![0u32; 30];
    for s in syms {
        match *s {
            Sym::Lit(b) => lf[b as usize] += 1,
            Sym::Match(l, d) => {
                lf[len_code(l).0] += 1;
                df[dist_code(d).0] += 1;
            }
        }
    }
    lf[256] = 1;
    let mut ll = huff_lengths(&lf, 15);
    let mut dl = huff_lengths(&df, 15);
    if dl.iter().all(|&l| l == 0) {
        dl[0] = 1;
    }
    // some decoders want at least two distance codes
    if dl.iter().filter(|&&l| l > 0).count() == 1 {
        let used = dl.iter().position(|&l| l > 0).unwrap();
        dl[if used == 0 { 1 } else { 0 }] = 1;
    }
    let hlit = (257..=286).rev().find(|&n| ll[n - 1] > 0).unwrap_or(257).max(257);
    let hdist = (1..=30).rev().find(|&n| dl[n - 1] > 0).unwrap_or(1);
    ll.truncate(hlit.max(257));
    let mut all: Vec<u8> = ll.clone();
    all.extend_from_slice(&dl[..hdist]);
    // run-length code the lengths
    let mut cl_syms: Vec<(u8, u32, u32)> = Vec::new(); // symbol, extra value, extra bits
    let mut i = 0;
    while i < all.len() {
        let v = all[i];
        let mut run = 1;
        while i + run < all.len() && all[i + run] == v {
            run += 1;
        }
        if v == 0 && run >= 3 {
            let r = run.min(138);
            if r >= 11 {
                cl_syms.push((18, (r - 11) as u32, 7));
            } else {
                cl_syms.push((17, (r - 3) as u32, 3));
            }
            i += r;
        } else if v != 0 && run >= 4 {
            cl_syms.push((v, 0, 0));
            let r = (run - 1).min(6);
            cl_syms.push((16, (r - 3) as u32, 2));
            i += 1 + r;
        } else {
            cl_syms.push((v, 0, 0));
            i += 1;
        }
    }
    let mut cf = vec![0u32; 19];
    for s in &cl_syms {
        cf[s.0 as usize] += 1;
    }
    let cll = huff_lengths(&cf, 7);
    let clc = codes(&cll);
    let hclen = (4..=19).rev().find(|&n| cll[CL_ORDER[n - 1]] > 0).unwrap_or(4);
    let lc = codes(&ll);
    let dc = codes(&dl);
    w.put(last as u32, 1);
    w.put(2, 2);
    w.put((hlit - 257) as u32, 5);
    w.put((hdist - 1) as u32, 5);
    w.put((hclen - 4) as u32, 4);
    for &o in CL_ORDER.iter().take(hclen) {
        w.put(cll[o] as u32, 3);
    }
    for s in &cl_syms {
        w.put(clc[s.0 as usize], cll[s.0 as usize] as u32);
        if s.2 > 0 {
            w.put(s.1, s.2);
        }
    }
    for s in syms {
        match *s {
            Sym::Lit(b) => w.put(lc[b as usize], ll[b as usize] as u32),
            Sym::Match(l, d) => {
                let (c, ev, eb) = len_code(l);
                w.put(lc[c], ll[c] as u32);
                if eb > 0 {
                    w.put(ev, eb);
                }
                let (c, ev, eb) = dist_code(d);
                w.put(dc[c], dl[c] as u32);
                if eb > 0 {
                    w.put(ev, eb);
                }
            }
        }
    }
    w.put(lc[256], ll[256] as u32);
}

/// Raw deflate.
pub fn deflate(data: &[u8]) -> Vec<u8> {
    const WSIZE: usize = 1 << 15;
    const HBITS: u32 = 15;
    const MAX_CHAIN: usize = 48;
    const NICE: usize = 128;
    let mut w = BitOut { out: Vec::with_capacity(data.len() / 3 + 64), buf: 0, n: 0 };
    if data.is_empty() {
        emit_block(&mut w, &[], true);
        w.flush();
        return w.out;
    }
    let mut head = vec![usize::MAX; 1 << HBITS];
    let mut prev = vec![usize::MAX; WSIZE];
    let hash = |p: usize| -> usize {
        let v = (data[p] as u32) | (data[p + 1] as u32) << 8 | (data[p + 2] as u32) << 16;
        (v.wrapping_mul(0x9E37_79B1) >> (32 - HBITS)) as usize
    };
    let n = data.len();
    let insert = |p: usize, head: &mut Vec<usize>, prev: &mut Vec<usize>| {
        if p + 2 < n {
            let h = hash(p);
            prev[p & (WSIZE - 1)] = head[h];
            head[h] = p;
        }
    };
    let longest = |p: usize, head: &Vec<usize>, prev: &Vec<usize>| -> (usize, usize) {
        if p + 2 >= n {
            return (0, 0);
        }
        let mut cand = head[hash(p)];
        let (mut best, mut bd) = (0usize, 0usize);
        let maxlen = (n - p).min(258);
        let mut chain = 0;
        while cand != usize::MAX && p - cand < WSIZE && p > cand && chain < MAX_CHAIN {
            if data[cand + best.min(maxlen - 1)] == data[p + best.min(maxlen - 1)] {
                let mut l = 0;
                while l < maxlen && data[cand + l] == data[p + l] {
                    l += 1;
                }
                if l > best {
                    best = l;
                    bd = p - cand;
                    if l >= NICE || l == maxlen {
                        break;
                    }
                }
            }
            let nx = prev[cand & (WSIZE - 1)];
            if nx == usize::MAX || nx >= cand {
                break;
            }
            cand = nx;
            chain += 1;
        }
        (best, bd)
    };
    let mut syms: Vec<Sym> = Vec::with_capacity(1 << 16);
    let mut p = 0;
    while p < n {
        let (l, d) = longest(p, &head, &prev);
        insert(p, &mut head, &mut prev);
        if l >= 3 {
            // one step of lazy matching
            let (l2, _) = if l < NICE && p + 1 < n { longest(p + 1, &head, &prev) } else { (0, 0) };
            if l2 > l {
                syms.push(Sym::Lit(data[p]));
                p += 1;
            } else {
                syms.push(Sym::Match(l as u16, d as u16));
                for q in p + 1..p + l {
                    insert(q, &mut head, &mut prev);
                }
                p += l;
            }
        } else {
            syms.push(Sym::Lit(data[p]));
            p += 1;
        }
        if syms.len() >= 1 << 16 {
            emit_block(&mut w, &syms, false);
            syms.clear();
        }
    }
    emit_block(&mut w, &syms, true);
    w.flush();
    w.out
}

/// zlib framing around deflate.
pub fn zlib(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x9c];
    out.extend(deflate(data));
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(n: usize, seed: u64) -> Vec<u8> {
        let mut s = seed;
        let mut v = Vec::with_capacity(n);
        for i in 0..n {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            // mixture of runs, repeats and noise
            let b = match (i / 997) % 3 {
                0 => (s >> 60) as u8,
                1 => (i % 7) as u8 * 13,
                _ => (s >> 33) as u8,
            };
            v.push(b);
        }
        v
    }

    #[test]
    fn round_trip() {
        for (n, seed) in [(0, 1), (1, 2), (300, 3), (70_000, 4), (400_000, 5)] {
            let d = sample(n, seed);
            let z = zlib(&d);
            assert_eq!(unzlib(&z).unwrap(), d, "n={n}");
        }
        let flat = vec![7u8; 200_000];
        let z = zlib(&flat);
        assert!(z.len() < 2_000, "a flat run compresses ({} bytes)", z.len());
        assert_eq!(unzlib(&z).unwrap(), flat);
    }

    #[test]
    fn inflates_fixed_and_stored_blocks() {
        // "hello hello hello" deflated by zlib with fixed Huffman codes (python zlib.compress(b'hello hello hello', 9))
        let z = [0x78, 0xda, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x90, 0x00, 0x3a, 0x2e, 0x06, 0x7d];
        assert_eq!(unzlib(&z).unwrap(), b"hello hello hello");
        // a stored block: BFINAL=1, BTYPE=00, LEN=3, NLEN=!3, "abc"
        let raw = [0x01, 0x03, 0x00, 0xfc, 0xff, b'a', b'b', b'c'];
        assert_eq!(inflate(&raw).unwrap(), b"abc");
    }
}
