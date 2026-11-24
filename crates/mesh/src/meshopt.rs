//! meshoptimizer's vertex and index codecs, decoded natively.
//!
//! Source 2 stores a compiled model's vertex and index buffers in `MVTX` / `MIDX` blocks, and
//! `m_bMeshoptCompressed` says they went through meshoptimizer's encoders. Build 6864 writes the
//! vertex codec at version 1 (header `0xA1`) and the index codec at version 1 (`0xE1`); version 0
//! (`0xA0`, `0xE0`) is read too, and the index-sequence codec (`0xD0`/`0xD1`) for buffers flagged
//! `m_bMeshoptIndexSequence`.
//!
//! Vertex codec. The buffer is cut into blocks of `getVertexBlockSize(vertex_size)` vertices
//! (8192 / size, rounded down to 16, at most 256). Inside a block every byte column is stored as a
//! byte plane of deltas against the previous vertex (the first block against the "tail" vertex at
//! the end of the stream). Planes are cut into groups of 16 bytes; a 2-bit header per group picks
//! how many bits each value takes (v0: 0/2/4/8; v1: 0/1/2/4 or 1/2/4/8 depending on a per-byte
//! control); a value equal to the all-ones code is an escape to a full byte stored after the
//! group. Version 1 adds, per 4-byte column, a control byte (per byte: bit table A, bit table B,
//! all zero, literal) and a channel mode in the tail (byte deltas, 16-bit deltas, or 32-bit XOR
//! with a rotation), so floats compress as words rather than bytes.
//!
//! Index codec. Triangles are coded against a 16-entry edge FIFO and a 16-entry vertex FIFO: one
//! code byte per triangle (`0xX0..0xEF`: an edge from the FIFO plus a new, cached or explicit third
//! vertex; `0xF0..0xFD`: a triangle with a "next" vertex and two from a 16-entry aux table stored
//! at the very end; `0xFE`/`0xFF`: explicit), explicit indices as zigzag deltas in LEB128 bytes.
//! Version 1 spends FIFO codes 13 and 14 on "last ±1".
//!
//! Every decoder here is total: malformed input is an `Err` naming what ran out, never a panic.

pub type Result<T> = std::result::Result<T, String>;

const VERTEX_HEADER: u8 = 0xa0;
const INDEX_HEADER: u8 = 0xe0;
const SEQUENCE_HEADER: u8 = 0xd0;
const BLOCK_SIZE_BYTES: usize = 8192;
const BLOCK_MAX: usize = 256;
const GROUP: usize = 16;
const GROUP_DECODE_LIMIT: usize = 24;
const TAIL_MIN_V0: usize = 32;
const TAIL_MIN_V1: usize = 24;
const BITS_V0: [u32; 4] = [0, 2, 4, 8];
const BITS_V1: [u32; 5] = [0, 1, 2, 4, 8];

/// Vertices per block for a vertex of `size` bytes (part of the format).
pub fn vertex_block_size(size: usize) -> usize {
    let r = (BLOCK_SIZE_BYTES / size) & !(GROUP - 1);
    r.min(BLOCK_MAX)
}

/// The codec version of an encoded vertex buffer, if the header is a vertex header.
pub fn vertex_version(buf: &[u8]) -> Option<u8> {
    buf.first().filter(|h| *h & 0xf0 == VERTEX_HEADER).map(|h| h & 0x0f)
}

/// The codec version of an encoded index buffer.
pub fn index_version(buf: &[u8]) -> Option<u8> {
    buf.first().filter(|h| *h & 0xf0 == INDEX_HEADER).map(|h| h & 0x0f)
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
    end: usize,
}

impl<'a> Reader<'a> {
    fn left(&self) -> usize {
        self.end - self.p
    }
}

/// One group of 16 values at `bits` per value (escapes read after the packed bits).
fn decode_group(r: &mut Reader, out: &mut [u8], bits: u32) -> Result<()> {
    match bits {
        0 => {
            out[..GROUP].fill(0);
            Ok(())
        }
        8 => {
            if r.left() < GROUP {
                return Err("literal group past the end".into());
            }
            out[..GROUP].copy_from_slice(&r.b[r.p..r.p + GROUP]);
            r.p += GROUP;
            Ok(())
        }
        1 | 2 | 4 => {
            let packed = (GROUP * bits as usize) / 8;
            if r.left() < packed {
                return Err("bit group past the end".into());
            }
            let head = r.p;
            let mut var = r.p + packed;
            let esc = (1u32 << bits) - 1;
            let per = 8 / bits as usize;
            for i in 0..GROUP {
                let byte = r.b[head + i / per] as u32;
                let slot = i % per;
                // v1's 1-bit groups store the lowest bit first; 2- and 4-bit groups the highest first.
                let enc = if bits == 1 { (byte >> slot) & 1 } else { (byte >> (8 - bits as usize * (slot + 1))) & esc };
                out[i] = if enc == esc {
                    if var >= r.end {
                        return Err("escape byte past the end".into());
                    }
                    var += 1;
                    r.b[var - 1]
                } else {
                    enc as u8
                };
            }
            r.p = var;
            Ok(())
        }
        _ => Err(format!("bad bit width {bits}")),
    }
}

/// A byte plane of `size` (a multiple of 16) values: a 2-bit header per group, then the groups.
fn decode_bytes(r: &mut Reader, out: &mut [u8], size: usize, table: &[u32]) -> Result<()> {
    let header_size = (size / GROUP).div_ceil(4);
    if r.left() < header_size {
        return Err("byte header past the end".into());
    }
    let header = r.p;
    r.p += header_size;
    let mut i = 0;
    while i < size {
        if r.left() < GROUP_DECODE_LIMIT {
            return Err("byte group too close to the end".into());
        }
        let g = i / GROUP;
        let sel = (r.b[header + g / 4] >> ((g % 4) * 2)) & 3;
        decode_group(r, &mut out[i..i + GROUP], table[sel as usize])?;
        i += GROUP;
    }
    Ok(())
}

fn unzigzag8(v: u8) -> u8 {
    (0u8.wrapping_sub(v & 1)) ^ (v >> 1)
}

fn unzigzag16(v: u16) -> u16 {
    (0u16.wrapping_sub(v & 1)) ^ (v >> 1)
}

fn rotate(v: u32, r: u32) -> u32 {
    v.rotate_left(r)
}

/// Decode a meshoptimizer vertex buffer (`meshopt_decodeVertexBuffer`) into `count * size` bytes.
pub fn decode_vertex_buffer(count: usize, size: usize, buf: &[u8]) -> Result<Vec<u8>> {
    if size == 0 || size > 256 {
        return Err(format!("vertex size {size} outside 1..256"));
    }
    let version = vertex_version(buf).ok_or("not a meshoptimizer vertex buffer")?;
    if version > 1 {
        return Err(format!("vertex codec version {version} (this decoder reads 0 and 1)"));
    }
    if version == 1 && !size.is_multiple_of(4) {
        return Err(format!("vertex codec v1 needs a size that is a multiple of 4, got {size}"));
    }
    let end = buf.len();
    let tail_size = size + if version == 0 { 0 } else { size / 4 };
    let tail_min = if version == 0 { TAIL_MIN_V0 } else { TAIL_MIN_V1 };
    let tail_pad = tail_size.max(tail_min);
    if end < 1 + tail_pad {
        return Err("buffer shorter than its tail".into());
    }
    let tail = end - tail_size;
    let mut last = [0u8; 256];
    last[..size].copy_from_slice(&buf[tail..tail + size]);
    let channels: &[u8] = if version == 0 { &[] } else { &buf[tail + size..tail + size + size / 4] };

    // `count` is the model's word: every block of vertices costs at least one header byte per byte channel (v0) or
    // its control bytes (v1), so a count the buffer could not hold is refused before it sizes the output.
    let block = vertex_block_size(size);
    let per_block = if version == 0 { size } else { size / 4 };
    let total = count.checked_mul(size).ok_or_else(|| format!("{count} vertices of {size} bytes"))?;
    if count.div_ceil(block).saturating_mul(per_block) > end {
        return Err(format!("{count} vertices need more than the {end}-byte buffer holds"));
    }
    let mut out = vec![0u8; total];
    let mut r = Reader { b: buf, p: 1, end };
    let mut plane = vec![0u8; BLOCK_MAX * 4 + GROUP];
    let mut transposed = vec![0u8; BLOCK_SIZE_BYTES];
    let mut offset = 0;
    while offset < count {
        let n = block.min(count - offset);
        let aligned = (n + GROUP - 1) & !(GROUP - 1);
        if version == 0 {
            for k in 0..size {
                decode_bytes(&mut r, &mut plane, aligned, &BITS_V0)?;
                let mut p = last[k];
                for i in 0..n {
                    let v = unzigzag8(plane[i]).wrapping_add(p);
                    transposed[k + i * size] = v;
                    p = v;
                }
            }
        } else {
            let control_size = size / 4;
            if r.left() < control_size {
                return Err("control bytes past the end".into());
            }
            let control = r.p;
            r.p += control_size;
            for k in (0..size).step_by(4) {
                let ctrl_byte = buf[control + k / 4];
                for j in 0..4 {
                    let ctrl = (ctrl_byte >> (j * 2)) & 3;
                    let dst = j * n;
                    match ctrl {
                        3 => {
                            if r.left() < n {
                                return Err("literal plane past the end".into());
                            }
                            plane[dst..dst + n].copy_from_slice(&buf[r.p..r.p + n]);
                            r.p += n;
                        }
                        2 => plane[dst..dst + n].fill(0),
                        _ => {
                            let mut tmp = [0u8; BLOCK_MAX + GROUP];
                            decode_bytes(&mut r, &mut tmp, aligned, &BITS_V1[ctrl as usize..])?;
                            plane[dst..dst + n].copy_from_slice(&tmp[..n]);
                        }
                    }
                }
                let channel = channels[k / 4];
                match channel & 3 {
                    0 => {
                        for b in 0..4 {
                            let mut p = last[k + b];
                            for i in 0..n {
                                let v = unzigzag8(plane[b * n + i]).wrapping_add(p);
                                transposed[k + b + i * size] = v;
                                p = v;
                            }
                        }
                    }
                    1 => {
                        for h in 0..2 {
                            let mut p = u16::from_le_bytes([last[k + 2 * h], last[k + 2 * h + 1]]);
                            for i in 0..n {
                                let raw = plane[(2 * h) * n + i] as u16 | (plane[(2 * h + 1) * n + i] as u16) << 8;
                                let v = unzigzag16(raw).wrapping_add(p);
                                transposed[k + 2 * h + i * size..k + 2 * h + i * size + 2].copy_from_slice(&v.to_le_bytes());
                                p = v;
                            }
                        }
                    }
                    2 => {
                        let rot = (32 - (channel as u32 >> 4)) & 31;
                        let mut p = u32::from_le_bytes([last[k], last[k + 1], last[k + 2], last[k + 3]]);
                        for i in 0..n {
                            let raw = plane[i] as u32 | (plane[n + i] as u32) << 8 | (plane[2 * n + i] as u32) << 16 | (plane[3 * n + i] as u32) << 24;
                            let v = rotate(raw, rot) ^ p;
                            transposed[k + i * size..k + i * size + 4].copy_from_slice(&v.to_le_bytes());
                            p = v;
                        }
                    }
                    _ => return Err(format!("invalid channel mode {channel:#x}")),
                }
            }
        }
        out[offset * size..(offset + n) * size].copy_from_slice(&transposed[..n * size]);
        last[..size].copy_from_slice(&transposed[(n - 1) * size..n * size]);
        offset += n;
    }
    if end - r.p != tail_pad {
        return Err(format!("{} bytes left before the tail, expected {tail_pad}", end - r.p));
    }
    Ok(out)
}

fn vbyte(b: &[u8], p: &mut usize, end: usize) -> Result<u32> {
    let mut result = 0u32;
    let mut shift = 0;
    for _ in 0..5 {
        if *p >= end {
            return Err("index data ran out".into());
        }
        let g = b[*p];
        *p += 1;
        result |= ((g & 127) as u32) << shift;
        if g < 128 {
            return Ok(result);
        }
        shift += 7;
    }
    Ok(result)
}

fn delta_index(b: &[u8], p: &mut usize, end: usize, last: u32) -> Result<u32> {
    let v = vbyte(b, p, end)?;
    let d = (v >> 1) ^ (0u32.wrapping_sub(v & 1));
    Ok(last.wrapping_add(d))
}

/// Decode a meshoptimizer index buffer (`meshopt_decodeIndexBuffer`): triangles, `count` indices.
pub fn decode_index_buffer(count: usize, buf: &[u8]) -> Result<Vec<u32>> {
    if !count.is_multiple_of(3) {
        return Err(format!("index count {count} is not a multiple of 3"));
    }
    let version = index_version(buf).ok_or("not a meshoptimizer index buffer")?;
    if version > 1 {
        return Err(format!("index codec version {version} (this decoder reads 0 and 1)"));
    }
    if buf.len() < 1 + count / 3 + 16 {
        return Err("index buffer shorter than its code stream".into());
    }
    let safe_end = buf.len() - 16;
    let aux = &buf[safe_end..];
    let fecmax: u32 = if version >= 1 { 13 } else { 15 };
    let mut edge = [[0u32; 2]; 16];
    let mut vfifo = [0u32; 16];
    let (mut eo, mut vo) = (0usize, 0usize);
    let (mut next, mut last) = (0u32, 0u32);
    let mut code = 1usize;
    let mut data = 1 + count / 3;
    let mut out = Vec::with_capacity(count);
    let push_v = |f: &mut [u32; 16], o: &mut usize, v: u32, cond: bool| {
        f[*o] = v;
        *o = (*o + cond as usize) & 15;
    };
    let push_e = |f: &mut [[u32; 2]; 16], o: &mut usize, a: u32, b: u32| {
        f[*o] = [a, b];
        *o = (*o + 1) & 15;
    };
    for _ in 0..count / 3 {
        let codetri = buf[code] as u32;
        code += 1;
        if codetri < 0xf0 {
            let fe = (codetri >> 4) as usize;
            let [a, b] = edge[(eo.wrapping_sub(1 + fe)) & 15];
            let fec = codetri & 15;
            if fec < fecmax {
                let cf = vfifo[(vo.wrapping_sub(1 + fec as usize)) & 15];
                let c = if fec == 0 { next } else { cf };
                let fec0 = fec == 0;
                next += fec0 as u32;
                out.extend_from_slice(&[a, b, c]);
                push_v(&mut vfifo, &mut vo, c, fec0);
                push_e(&mut edge, &mut eo, c, b);
                push_e(&mut edge, &mut eo, a, c);
            } else {
                let c = if fec != 15 {
                    // 13 and 14 decode to last - 1 and last + 1
                    last.wrapping_add((fec as i32 - (fec ^ 3) as i32) as u32)
                } else {
                    delta_index(buf, &mut data, safe_end, last)?
                };
                last = c;
                out.extend_from_slice(&[a, b, c]);
                push_v(&mut vfifo, &mut vo, c, true);
                push_e(&mut edge, &mut eo, c, b);
                push_e(&mut edge, &mut eo, a, c);
            }
        } else if codetri < 0xfe {
            let codeaux = aux[(codetri & 15) as usize] as u32;
            let feb = (codeaux >> 4) as usize;
            let fec = (codeaux & 15) as usize;
            let a = next;
            next += 1;
            let b = if feb == 0 { next } else { vfifo[(vo.wrapping_sub(feb)) & 15] };
            let feb0 = feb == 0;
            next += feb0 as u32;
            let c = if fec == 0 { next } else { vfifo[(vo.wrapping_sub(fec)) & 15] };
            let fec0 = fec == 0;
            next += fec0 as u32;
            out.extend_from_slice(&[a, b, c]);
            push_v(&mut vfifo, &mut vo, a, true);
            push_v(&mut vfifo, &mut vo, b, feb0);
            push_v(&mut vfifo, &mut vo, c, fec0);
            push_e(&mut edge, &mut eo, b, a);
            push_e(&mut edge, &mut eo, c, b);
            push_e(&mut edge, &mut eo, a, c);
        } else {
            if data >= safe_end {
                return Err("index data ran out".into());
            }
            let codeaux = buf[data] as u32;
            data += 1;
            let fea = if codetri == 0xfe { 0 } else { 15 };
            let feb = (codeaux >> 4) as usize;
            let fec = (codeaux & 15) as usize;
            if codeaux == 0 {
                next = 0;
            }
            let mut a = if fea == 0 {
                next += 1;
                next - 1
            } else {
                0
            };
            let mut b = if feb == 0 {
                next += 1;
                next - 1
            } else {
                vfifo[(vo.wrapping_sub(feb)) & 15]
            };
            let mut c = if fec == 0 {
                next += 1;
                next - 1
            } else {
                vfifo[(vo.wrapping_sub(fec)) & 15]
            };
            if fea == 15 {
                a = delta_index(buf, &mut data, safe_end, last)?;
                last = a;
            }
            if feb == 15 {
                b = delta_index(buf, &mut data, safe_end, last)?;
                last = b;
            }
            if fec == 15 {
                c = delta_index(buf, &mut data, safe_end, last)?;
                last = c;
            }
            out.extend_from_slice(&[a, b, c]);
            push_v(&mut vfifo, &mut vo, a, true);
            push_v(&mut vfifo, &mut vo, b, feb == 0 || feb == 15);
            push_v(&mut vfifo, &mut vo, c, fec == 0 || fec == 15);
            push_e(&mut edge, &mut eo, b, a);
            push_e(&mut edge, &mut eo, c, b);
            push_e(&mut edge, &mut eo, a, c);
        }
    }
    if data != safe_end {
        return Err(format!("{} index bytes unread", safe_end as i64 - data as i64));
    }
    Ok(out)
}

/// Decode a meshoptimizer index sequence (`meshopt_decodeIndexSequence`): any index list.
pub fn decode_index_sequence(count: usize, buf: &[u8]) -> Result<Vec<u32>> {
    let h = *buf.first().ok_or("empty index sequence")?;
    if h & 0xf0 != SEQUENCE_HEADER || h & 0x0f > 1 {
        return Err(format!("not a meshoptimizer index sequence (header {h:#04x})"));
    }
    if buf.len() < 1 + count + 4 {
        return Err("index sequence too short".into());
    }
    let safe_end = buf.len() - 4;
    let mut last = [0u32; 2];
    let mut p = 1;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let v = vbyte(buf, &mut p, safe_end)?;
        let current = (v & 1) as usize;
        let v = v >> 1;
        let d = (v >> 1) ^ (0u32.wrapping_sub(v & 1));
        let idx = last[current].wrapping_add(d);
        last[current] = idx;
        out.push(idx);
    }
    if p != safe_end {
        return Err("index sequence has trailing bytes".into());
    }
    Ok(out)
}

/// Encode a vertex buffer with codec version 0 (the decoder's fixture source: every group is
/// coded at the smallest width that fits, so all four group widths and escapes are exercised).
pub fn encode_vertex_buffer_v0(data: &[u8], count: usize, size: usize) -> Vec<u8> {
    assert!(size > 0 && size <= 256 && data.len() == count * size);
    let mut out = vec![VERTEX_HEADER];
    let mut last = vec![0u8; size];
    if count > 0 {
        last.copy_from_slice(&data[..size]);
    }
    let first = last.clone();
    let block = vertex_block_size(size);
    let mut offset = 0;
    while offset < count {
        let n = block.min(count - offset);
        let aligned = (n + GROUP - 1) & !(GROUP - 1);
        for k in 0..size {
            let mut plane = vec![0u8; aligned];
            let mut p = last[k];
            for i in 0..n {
                let v = data[(offset + i) * size + k];
                let d = v.wrapping_sub(p) as i8;
                plane[i] = ((d as i32) << 1 ^ ((d as i32) >> 7)) as u8;
                p = v;
            }
            let groups = aligned / GROUP;
            let mut header = vec![0u8; groups.div_ceil(4)];
            let mut body = Vec::new();
            for g in 0..groups {
                let vals = &plane[g * GROUP..g * GROUP + GROUP];
                let sel = if vals.iter().all(|&v| v == 0) {
                    0
                } else if vals.iter().filter(|&&v| v >= 3).count() <= 2 {
                    1
                } else if vals.iter().filter(|&&v| v >= 15).count() <= 4 {
                    2
                } else {
                    3
                };
                header[g / 4] |= (sel as u8) << ((g % 4) * 2);
                let bits = BITS_V0[sel];
                match bits {
                    0 => {}
                    8 => body.extend_from_slice(vals),
                    _ => {
                        let esc = (1u8 << bits) - 1;
                        let per = 8 / bits as usize;
                        let mut packed = vec![0u8; GROUP / per];
                        let mut extra = Vec::new();
                        for (i, &v) in vals.iter().enumerate() {
                            let enc = if v >= esc {
                                extra.push(v);
                                esc
                            } else {
                                v
                            };
                            packed[i / per] |= enc << (8 - bits as usize * (i % per + 1));
                        }
                        body.extend_from_slice(&packed);
                        body.extend_from_slice(&extra);
                    }
                }
            }
            out.extend_from_slice(&header);
            out.extend_from_slice(&body);
        }
        last.copy_from_slice(&data[(offset + n - 1) * size..(offset + n) * size]);
        offset += n;
    }
    let tail_pad = size.max(TAIL_MIN_V0);
    out.extend(std::iter::repeat_n(0, tail_pad - size));
    out.extend_from_slice(&first);
    out
}

/// Encode triangles with codec version 1 using only the explicit path (`0xFF` code, three
/// delta-coded indices): valid input for `decode_index_buffer` that exercises its explicit branch.
pub fn encode_index_buffer_explicit(indices: &[u32]) -> Vec<u8> {
    assert!(indices.len().is_multiple_of(3));
    let mut out = vec![INDEX_HEADER | 1];
    out.extend(std::iter::repeat_n(0xff, indices.len() / 3));
    let mut last = 0u32;
    for t in indices.chunks(3) {
        out.push(0xff);
        for &i in t {
            let d = i.wrapping_sub(last) as i32;
            let mut v = ((d << 1) ^ (d >> 31)) as u32;
            loop {
                let g = (v & 127) as u8;
                v >>= 7;
                if v == 0 {
                    out.push(g);
                    break;
                }
                out.push(g | 128);
            }
            last = i;
        }
    }
    out.extend(std::iter::repeat_n(0, 16));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_size_matches_the_format() {
        assert_eq!(vertex_block_size(28), 256);
        assert_eq!(vertex_block_size(24), 256 & !15);
        assert_eq!(vertex_block_size(64), 128);
        assert_eq!(vertex_block_size(100), 80);
    }

    #[test]
    fn vertex_v0_round_trip_all_group_widths() {
        let (count, size) = (700, 12);
        let mut data = vec![0u8; count * size];
        let mut s = 12345u32;
        for i in 0..count {
            for k in 0..size {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                data[i * size + k] = match k % 4 {
                    0 => (i / 7) as u8,                 // slow ramp: tiny deltas
                    1 => 7,                             // constant: zero groups
                    2 => (s >> 24) as u8,               // noise: literal groups
                    _ => (i as u8).wrapping_mul(3) ^ ((s >> 30) as u8), // mid
                };
            }
        }
        let enc = encode_vertex_buffer_v0(&data, count, size);
        assert_eq!(decode_vertex_buffer(count, size, &enc).unwrap(), data);
    }

    #[test]
    fn index_explicit_round_trip() {
        let idx = vec![0, 1, 2, 2, 1, 3, 100, 5, 70000, 3, 3, 3];
        let enc = encode_index_buffer_explicit(&idx);
        assert_eq!(decode_index_buffer(idx.len(), &enc).unwrap(), idx);
    }

    #[test]
    fn decoders_refuse_garbage() {
        assert!(decode_vertex_buffer(10, 12, &[0x55; 40]).is_err());
        assert!(decode_vertex_buffer(10, 12, &[0xa0; 5]).is_err());
        assert!(decode_index_buffer(3, &[0xe0, 0x00]).is_err());
        assert!(decode_index_sequence(3, &[0x00]).is_err());
    }
}
