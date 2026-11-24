//! Hand-written JSON, the one writer every machine-readable output uses (the grade record here, and `magician`'s
//! `--json` documents through `jsonout.rs`). Zero crates: strings are escaped here, and a number prints the same text
//! every time, so a record is byte-for-byte reproducible.

/// A JSON string literal, quotes included.
pub fn s(text: &str) -> String {
    let mut o = String::with_capacity(text.len() + 2);
    o.push('"');
    let q = char::from(92u8);
    for c in text.chars() {
        match c as u32 {
            34 => {
                o.push(q);
                o.push('"');
            }
            92 => {
                o.push(q);
                o.push(q);
            }
            10 => {
                o.push(q);
                o.push('n');
            }
            13 => {
                o.push(q);
                o.push('r');
            }
            9 => {
                o.push(q);
                o.push('t');
            }
            n if n < 32 => o.push_str(&format!("{q}u{n:04x}")),
            _ => o.push(c),
        }
    }
    o.push('"');
    o
}

/// A number as JSON: whole numbers without a fraction, others to at most four decimals with trailing zeros cut, and
/// `null` for NaN or infinity. The same value always prints the same text.
pub fn num(x: f64) -> String {
    if !x.is_finite() {
        return "null".into();
    }
    let t = format!("{:.4}", x);
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t == "-0" { "0".into() } else { t.to_string() }
}

/// A JSON array of already-encoded items.
pub fn arr<I: IntoIterator<Item = String>>(items: I) -> String {
    format!("[{}]", items.into_iter().collect::<Vec<_>>().join(","))
}

/// A JSON array of strings.
pub fn strs<S: AsRef<str>>(items: &[S]) -> String {
    arr(items.iter().map(|x| s(x.as_ref())))
}

/// A JSON object built field by field, in the order the fields are added.
#[derive(Default)]
pub struct Obj(pub Vec<(String, String)>);

impl Obj {
    pub fn new() -> Obj {
        Obj::default()
    }
    /// A field whose value is already JSON.
    pub fn raw(mut self, k: &str, v: String) -> Obj {
        self.0.push((k.to_string(), v));
        self
    }
    pub fn str(self, k: &str, v: &str) -> Obj {
        self.raw(k, s(v))
    }
    pub fn opt_str(self, k: &str, v: Option<&str>) -> Obj {
        self.raw(k, v.map_or("null".into(), s))
    }
    pub fn num(self, k: &str, v: f64) -> Obj {
        self.raw(k, num(v))
    }
    pub fn int(self, k: &str, v: i64) -> Obj {
        self.raw(k, v.to_string())
    }
    pub fn bool(self, k: &str, v: bool) -> Obj {
        self.raw(k, v.to_string())
    }
    pub fn strs<S: AsRef<str>>(self, k: &str, v: &[S]) -> Obj {
        self.raw(k, strs(v))
    }
    pub fn build(&self) -> String {
        format!("{{{}}}", self.0.iter().map(|(k, v)| format!("{}:{v}", s(k))).collect::<Vec<_>>().join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes() {
        let (q, dq) = (char::from(92u8), char::from(34u8));
        let input: String = [97u8, 34, 92, 10, 9, 1].iter().map(|&b| char::from(b)).collect();
        let want = format!("{dq}a{q}{dq}{q}{q}{q}n{q}t{q}u0001{dq}");
        assert_eq!(s(&input), want);
    }

    #[test]
    fn numbers_print_one_way() {
        assert_eq!(num(90.0), "90");
        assert_eq!(num(91.25), "91.25");
        assert_eq!(num(0.0350000001), "0.035");
        assert_eq!(num(-0.00001), "0");
        assert_eq!(num(f64::NAN), "null");
        assert_eq!(Obj::new().str("a", "x").num("b", 1.5).bool("c", true).opt_str("d", None).strs("e", &["p", "q"]).build(), "{\"a\":\"x\",\"b\":1.5,\"c\":true,\"d\":null,\"e\":[\"p\",\"q\"]}");
    }
}
