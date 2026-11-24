//! LZ4 block decompression (the raw block format, no frame header). KV3 blocks in compiled resources use
//! it for their main buffers and, chained in fixed-size frames, for their binary blobs.

/// Decompress one LZ4 block, appending to `out`. Matches may reach back into bytes already in `out`, which
/// is what a chained ("continue") decoder needs: decoding consecutive frames into one buffer gives each
/// frame the previous ones as its dictionary.
pub fn decompress_into(src: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    let mut i = 0usize;
    let err = |what: &str| Err(format!("lz4: {what}"));
    while i < src.len() {
        let token = src[i];
        i += 1;
        let mut lit = (token >> 4) as usize;
        if lit == 15 {
            loop {
                let Some(&b) = src.get(i) else { return err("truncated literal length") };
                i += 1;
                lit += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        let Some(lits) = src.get(i..i + lit) else { return err("literals past end of input") };
        out.extend_from_slice(lits);
        i += lit;
        if i >= src.len() {
            break; // the last sequence carries literals only
        }
        let Some(off) = src.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize) else {
            return err("truncated match offset");
        };
        i += 2;
        if off == 0 || off > out.len() {
            return err("match offset outside the output");
        }
        let mut len = (token & 15) as usize;
        if len == 15 {
            loop {
                let Some(&b) = src.get(i) else { return err("truncated match length") };
                i += 1;
                len += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        len += 4;
        let start = out.len() - off;
        if off >= len {
            out.extend_from_within(start..start + len);
        } else {
            for k in 0..len {
                let b = out[start + k];
                out.push(b);
            }
        }
    }
    Ok(())
}

/// Decompress one LZ4 block that must decode to `expected` bytes. `expected` comes from the file, so it only sizes
/// the buffer as far as the input could fill it: LZ4 expands at most 255 times, plus a sequence's slack.
pub fn decompress(src: &[u8], expected: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(expected.min(src.len().saturating_mul(255).saturating_add(16)));
    decompress_into(src, &mut out)?;
    if out.len() != expected {
        return Err(format!("lz4: decoded {} bytes, expected {expected}", out.len()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_and_overlapping_match() {
        // "abc" then a match of length 9 at offset 3 (overlapping): "abcabcabcabc".
        let block = [0x35, b'a', b'b', b'c', 3, 0];
        assert_eq!(decompress(&block, 12).unwrap(), b"abcabcabcabc");
    }

    #[test]
    fn rejects_bad_offsets() {
        assert!(decompress(&[0x10, b'a', 9, 0], 5).is_err());
    }

    #[test]
    fn long_literal_run() {
        let mut block = vec![0xF0, 5];
        block.extend(std::iter::repeat_n(b'x', 20));
        assert_eq!(decompress(&block, 20).unwrap(), vec![b'x'; 20]);
    }
}
