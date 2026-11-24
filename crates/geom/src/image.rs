//! RGBA images, the drawing the reports need (rects, lines, text), and a PNG encoder with its own
//! deflate (fixed Huffman codes over hash-chain LZ77, zlib framing, CRC-32 and Adler-32), so x-ray
//! sheets are written with no dependency. Deterministic: no timestamps, fixed compression choices,
//! so the same model writes byte-identical PNGs (scene3d's sheet discipline).

use crate::font;

pub type Rgba = [u8; 4];

#[derive(Debug, Clone)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Image {
    pub fn new(w: usize, h: usize, bg: Rgba) -> Image {
        let mut px = Vec::with_capacity(w * h * 4);
        for _ in 0..w * h {
            px.extend_from_slice(&bg);
        }
        Image { w, h, px }
    }
    pub fn get(&self, x: usize, y: usize) -> Rgba {
        let i = (y * self.w + x) * 4;
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }
    pub fn set(&mut self, x: i64, y: i64, c: Rgba) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        if c[3] == 255 {
            self.px[i..i + 4].copy_from_slice(&c);
        } else {
            let a = c[3] as u32;
            for k in 0..3 {
                self.px[i + k] = ((self.px[i + k] as u32 * (255 - a) + c[k] as u32 * a) / 255) as u8;
            }
            self.px[i + 3] = self.px[i + 3].max(c[3]);
        }
    }
    pub fn rect(&mut self, x: i64, y: i64, w: i64, h: i64, c: Rgba) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }
    pub fn outline(&mut self, x: i64, y: i64, w: i64, h: i64, c: Rgba) {
        self.rect(x, y, w, 1, c);
        self.rect(x, y + h - 1, w, 1, c);
        self.rect(x, y, 1, h, c);
        self.rect(x + w - 1, y, 1, h, c);
    }
    pub fn line(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, c: Rgba) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            self.set(x, y, c);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }
    pub fn text(&mut self, x: i64, y: i64, s: &str, scale: usize, c: Rgba) {
        let mut cx = x;
        for ch in s.chars() {
            let rows = font::rows(ch);
            for (r, bits) in rows.iter().enumerate() {
                for col in 0..font::GLYPH_W {
                    if bits & (1 << (font::GLYPH_W - 1 - col)) != 0 {
                        self.rect(cx + (col * scale) as i64, y + (r * scale) as i64, scale as i64, scale as i64, c);
                    }
                }
            }
            cx += ((font::GLYPH_W + font::GLYPH_GAP) * scale) as i64;
        }
    }
    /// Copy `src` with its top-left at (x, y).
    pub fn blit(&mut self, src: &Image, x: i64, y: i64) {
        for yy in 0..src.h {
            for xx in 0..src.w {
                self.set(x + xx as i64, y + yy as i64, src.get(xx, yy));
            }
        }
    }
    pub fn png(&self) -> Vec<u8> {
        encode_png(self.w, self.h, &self.px)
    }
}

fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    for n in 0..256u32 {
        let mut c = n;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        t[n as usize] = c;
    }
    t
}

pub fn crc32(data: &[u8]) -> u32 {
    let t = crc_table();
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c = t[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    c ^ 0xffff_ffff
}

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

struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn put(&mut self, v: u32, n: u32) {
        self.acc |= (v as u64) << self.n;
        self.n += n;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    /// Huffman codes go most-significant bit first.
    fn code(&mut self, code: u32, len: u32) {
        let mut r = 0u32;
        for i in 0..len {
            r |= ((code >> i) & 1) << (len - 1 - i);
        }
        self.put(r, len);
    }
    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

fn lit(b: &mut Bits, v: u32) {
    match v {
        0..=143 => b.code(0x30 + v, 8),
        144..=255 => b.code(0x190 + (v - 144), 9),
        256..=279 => b.code(v - 256, 7),
        _ => b.code(0xc0 + (v - 280), 8),
    }
}

const LEN_BASE: [u32; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u32; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u32; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u32; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// Raw deflate: one fixed-Huffman block, greedy LZ77 over a 32 KiB window with hash chains.
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut b = Bits { out: Vec::with_capacity(data.len() / 4 + 64), acc: 0, n: 0 };
    b.put(1, 1); // BFINAL
    b.put(1, 2); // BTYPE = fixed
    const WIN: usize = 32768;
    const HBITS: u32 = 15;
    let mut head = vec![usize::MAX; 1 << HBITS];
    let mut prev = vec![usize::MAX; WIN];
    let hash = |i: usize| -> usize { ((data[i] as u32) << 10 ^ (data[i + 1] as u32) << 5 ^ data[i + 2] as u32).wrapping_mul(2654435761) as usize >> (32 - HBITS) & ((1 << HBITS) - 1) };
    let mut i = 0;
    let n = data.len();
    let insert = |pos: usize, head: &mut Vec<usize>, prev: &mut Vec<usize>| {
        if pos + 2 < n {
            let h = hash(pos);
            prev[pos % WIN] = head[h];
            head[h] = pos;
        }
    };
    while i < n {
        let mut best_len = 0;
        let mut best_dist = 0;
        if i + 2 < n {
            let mut cand = head[hash(i)];
            let mut chain = 64;
            while cand != usize::MAX && chain > 0 && i - cand < WIN {
                if cand < i {
                    let max = (n - i).min(258);
                    let mut l = 0;
                    while l < max && data[cand + l] == data[i + l] {
                        l += 1;
                    }
                    if l > best_len {
                        best_len = l;
                        best_dist = i - cand;
                        if l == max {
                            break;
                        }
                    }
                }
                let nx = prev[cand % WIN];
                if nx == usize::MAX || nx >= cand {
                    break;
                }
                cand = nx;
                chain -= 1;
            }
        }
        if best_len >= 3 {
            let li = LEN_BASE.iter().rposition(|&x| x as usize <= best_len).unwrap();
            lit(&mut b, 257 + li as u32);
            if LEN_EXTRA[li] > 0 {
                b.put(best_len as u32 - LEN_BASE[li], LEN_EXTRA[li]);
            }
            let di = DIST_BASE.iter().rposition(|&x| x as usize <= best_dist).unwrap();
            b.code(di as u32, 5);
            if DIST_EXTRA[di] > 0 {
                b.put(best_dist as u32 - DIST_BASE[di], DIST_EXTRA[di]);
            }
            for k in 0..best_len {
                insert(i + k, &mut head, &mut prev);
            }
            i += best_len;
        } else {
            lit(&mut b, data[i] as u32);
            insert(i, &mut head, &mut prev);
            i += 1;
        }
    }
    lit(&mut b, 256);
    b.finish()
}

pub fn zlib(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    out.extend(deflate(data));
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// PNG, 8-bit RGBA, every scanline with the Sub filter (cheap and good on flat masks).
pub fn encode_png(w: usize, h: usize, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(h * (w * 4 + 1));
    for y in 0..h {
        raw.push(1);
        let row = &rgba[y * w * 4..(y + 1) * w * 4];
        for x in 0..w * 4 {
            let left = if x >= 4 { row[x - 4] } else { 0 };
            raw.push(row[x].wrapping_sub(left));
        }
    }
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let chunk = |out: &mut Vec<u8>, tag: &[u8; 4], body: &[u8]| {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        let mut c = tag.to_vec();
        c.extend_from_slice(body);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Inflate (fixed and dynamic Huffman, stored blocks), for testing the encoder against an
/// independent decoder path.
pub fn inflate(src: &[u8]) -> Result<Vec<u8>, String> {
    struct R<'a> {
        b: &'a [u8],
        p: usize,
        bit: u32,
    }
    impl R<'_> {
        fn bits(&mut self, n: u32) -> Result<u32, String> {
            let mut v = 0;
            for i in 0..n {
                let byte = *self.b.get(self.p).ok_or("inflate ran out")?;
                v |= (((byte >> self.bit) & 1) as u32) << i;
                self.bit += 1;
                if self.bit == 8 {
                    self.bit = 0;
                    self.p += 1;
                }
            }
            Ok(v)
        }
    }
    fn build(lens: &[u8]) -> Vec<(u32, u32, u32)> {
        // (code, len, symbol), canonical
        let mut out = Vec::new();
        let mut code = 0u32;
        for l in 1..=15u8 {
            for (s, &x) in lens.iter().enumerate() {
                if x == l {
                    out.push((code, l as u32, s as u32));
                    code += 1;
                }
            }
            code <<= 1;
        }
        out
    }
    fn sym(r: &mut R, t: &[(u32, u32, u32)]) -> Result<u32, String> {
        let (mut code, mut len) = (0u32, 0u32);
        loop {
            code = (code << 1) | r.bits(1)?;
            len += 1;
            if let Some(&(_, _, s)) = t.iter().find(|&&(c, l, _)| l == len && c == code) {
                return Ok(s);
            }
            if len > 15 {
                return Err("bad huffman code".into());
            }
        }
    }
    let mut r = R { b: src, p: 0, bit: 0 };
    let mut out = Vec::new();
    loop {
        let fin = r.bits(1)?;
        let ty = r.bits(2)?;
        match ty {
            0 => {
                if r.bit > 0 {
                    r.bit = 0;
                    r.p += 1;
                }
                let len = u16::from_le_bytes([r.b[r.p], r.b[r.p + 1]]) as usize;
                r.p += 4;
                out.extend_from_slice(&r.b[r.p..r.p + len]);
                r.p += len;
            }
            1 | 2 => {
                let (lt, dt) = if ty == 1 {
                    let mut l = vec![8u8; 288];
                    l[144..256].iter_mut().for_each(|x| *x = 9);
                    l[256..280].iter_mut().for_each(|x| *x = 7);
                    (build(&l), build(&[5u8; 30]))
                } else {
                    let hlit = r.bits(5)? as usize + 257;
                    let hdist = r.bits(5)? as usize + 1;
                    let hclen = r.bits(4)? as usize + 4;
                    const ORD: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                    let mut cl = [0u8; 19];
                    for &o in ORD.iter().take(hclen) {
                        cl[o] = r.bits(3)? as u8;
                    }
                    let ct = build(&cl);
                    let mut lens = Vec::new();
                    while lens.len() < hlit + hdist {
                        match sym(&mut r, &ct)? {
                            s @ 0..=15 => lens.push(s as u8),
                            16 => {
                                let p = *lens.last().ok_or("repeat with no previous")?;
                                for _ in 0..3 + r.bits(2)? {
                                    lens.push(p);
                                }
                            }
                            17 => lens.extend(std::iter::repeat_n(0, 3 + r.bits(3)? as usize)),
                            _ => lens.extend(std::iter::repeat_n(0, 11 + r.bits(7)? as usize)),
                        }
                    }
                    (build(&lens[..hlit]), build(&lens[hlit..]))
                };
                loop {
                    let s = sym(&mut r, &lt)?;
                    if s < 256 {
                        out.push(s as u8);
                    } else if s == 256 {
                        break;
                    } else {
                        let li = (s - 257) as usize;
                        let len = LEN_BASE[li] + r.bits(LEN_EXTRA[li])?;
                        let di = sym(&mut r, &dt)? as usize;
                        let dist = DIST_BASE[di] + r.bits(DIST_EXTRA[di])?;
                        let start = out.len() - dist as usize;
                        for k in 0..len as usize {
                            out.push(out[start + k]);
                        }
                    }
                }
            }
            _ => return Err("bad block type".into()),
        }
        if fin == 1 {
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deflate_round_trips() {
        let mut data = Vec::new();
        for i in 0..20000u32 {
            data.push((i % 7) as u8);
            if i % 13 == 0 {
                data.push((i * 31) as u8);
            }
        }
        data.extend(std::iter::repeat_n(9u8, 5000));
        let z = deflate(&data);
        assert!(z.len() < data.len() / 4, "{} vs {}", z.len(), data.len());
        assert_eq!(inflate(&z).unwrap(), data);
        assert_eq!(inflate(&deflate(b"")).unwrap(), b"");
        assert_eq!(inflate(&deflate(b"ab")).unwrap(), b"ab");
    }

    #[test]
    fn known_checksums() {
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
    }

    #[test]
    fn png_is_deterministic_and_well_formed() {
        let mut im = Image::new(64, 32, [10, 20, 30, 255]);
        im.text(2, 2, "front 97%", 1, [255, 255, 255, 255]);
        im.line(0, 31, 63, 0, [255, 0, 0, 255]);
        let a = im.png();
        assert_eq!(a, im.png());
        assert_eq!(&a[1..4], b"PNG");
        // IDAT payload inflates to filtered rows of the right size
        let len = u32::from_be_bytes(a[33..37].try_into().unwrap()) as usize;
        let z = &a[41..41 + len];
        assert_eq!(inflate(&z[2..z.len() - 4]).unwrap().len(), 32 * (64 * 4 + 1));
    }
}
