//! Source 2 material expressions (`"DynamicParams"` in a .vmat): parse them and *prove* things about
//! them before they ship.
//!
//! The language, as Valve's own hero materials use it:
//! `v0 = .1;\nv4 = $ent_gametime-$UltCastStart;\nreturn lerp(0,4.2,clamp(v4/v0,0,1));`
//! - statements: `vN = expr;` and a final `return expr;`
//! - numbers (`.5`, `6.2832`, `1e-3`), attributes (`$AmmoPercent`, `$luminance.x`), locals
//! - `+ - * /`, unary `-`, comparisons, `? :`, `float2/3/4(...)`, swizzles (`.x`, `.xy`)
//! - functions: sin cos abs frac floor ceil sqrt min max clamp lerp smoothstep saturate pow
//!   random RemapVal time
//!
//! Expressions are pure functions evaluated every frame: no memory. "State" is derived from inputs
//! (time since a cast, ammo fraction). That is what makes them reliable, and what makes them provable:
//! every value is computed as an **interval** over all possible inputs, so a build can show that a
//! glow never exceeds a ceiling, nothing divides by zero, and the cost stays small.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fmt;

// ------------------------------------------------------------------ intervals

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Iv {
    pub lo: f64,
    pub hi: f64,
}

impl Iv {
    pub const ALL: Iv = Iv { lo: f64::NEG_INFINITY, hi: f64::INFINITY };
    pub fn pt(x: f64) -> Iv {
        Iv { lo: x, hi: x }
    }
    pub fn new(a: f64, b: f64) -> Iv {
        Iv { lo: a.min(b), hi: a.max(b) }
    }
    fn hull(a: Iv, b: Iv) -> Iv {
        Iv { lo: a.lo.min(b.lo), hi: a.hi.max(b.hi) }
    }
    fn of(vals: &[f64]) -> Iv {
        let finite: Vec<f64> = vals.iter().copied().map(|v| if v.is_nan() { f64::INFINITY } else { v }).collect();
        Iv {
            lo: finite.iter().copied().fold(f64::INFINITY, f64::min),
            hi: finite.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        }
    }
    pub fn bounded(&self) -> bool {
        self.lo.is_finite() && self.hi.is_finite()
    }
    fn contains0(&self) -> bool {
        self.lo <= 0.0 && self.hi >= 0.0
    }
    fn width(&self) -> f64 {
        self.hi - self.lo
    }
}

impl fmt::Display for Iv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = |x: f64| {
            if x == f64::INFINITY { "+inf".to_string() } else if x == f64::NEG_INFINITY { "-inf".to_string() } else { format!("{}", (x * 1e4).round() / 1e4) }
        };
        if self.lo == self.hi { write!(f, "{}", n(self.lo)) } else { write!(f, "[{}, {}]", n(self.lo), n(self.hi)) }
    }
}

fn mul(a: Iv, b: Iv) -> Iv {
    // 0 * inf is treated as 0: a zero factor annihilates (e.g. 0*time()).
    let m = |x: f64, y: f64| if x == 0.0 || y == 0.0 { 0.0 } else { x * y };
    Iv::of(&[m(a.lo, b.lo), m(a.lo, b.hi), m(a.hi, b.lo), m(a.hi, b.hi)])
}

fn monotone(a: Iv, f: impl Fn(f64) -> f64) -> Iv {
    Iv::new(f(a.lo), f(a.hi))
}

fn sin_iv(a: Iv) -> Iv {
    if !a.bounded() || a.width() >= std::f64::consts::TAU {
        return Iv::new(-1.0, 1.0);
    }
    let mut lo = a.lo.sin().min(a.hi.sin());
    let mut hi = a.lo.sin().max(a.hi.sin());
    // interior extrema at pi/2 + k*pi
    let k0 = ((a.lo - std::f64::consts::FRAC_PI_2) / std::f64::consts::PI).ceil() as i64;
    let k1 = ((a.hi - std::f64::consts::FRAC_PI_2) / std::f64::consts::PI).floor() as i64;
    for k in k0..=k1 {
        if k % 2 == 0 { hi = 1.0 } else { lo = -1.0 }
    }
    Iv { lo, hi }
}

// ------------------------------------------------------------------ values (1..4 components)

#[derive(Clone, Debug, PartialEq)]
pub struct Val(pub Vec<Iv>);

impl Val {
    fn s(i: Iv) -> Val {
        Val(vec![i])
    }
    fn comps(&self) -> usize {
        self.0.len()
    }
    fn comp(&self, i: usize) -> Iv {
        match self.0.len() {
            0 => Iv::new(f64::NEG_INFINITY, f64::INFINITY),
            1 => self.0[0],
            n => self.0[i.min(n - 1)],
        }
    }
    fn zip(a: &Val, b: &Val, f: impl Fn(Iv, Iv) -> Iv) -> Val {
        let n = a.comps().max(b.comps());
        Val((0..n).map(|i| f(a.comp(i), b.comp(i))).collect())
    }
    fn map(&self, f: impl Fn(Iv) -> Iv) -> Val {
        Val(self.0.iter().map(|&x| f(x)).collect())
    }
    pub fn range(&self) -> Iv {
        self.0.iter().copied().reduce(Iv::hull).unwrap_or(Iv::pt(0.0))
    }
}

impl fmt::Display for Val {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.len() == 1 {
            write!(f, "{}", self.0[0])
        } else {
            write!(f, "float{}(", self.0.len())?;
            for (i, v) in self.0.iter().enumerate() {
                if i > 0 { write!(f, ", ")? }
                write!(f, "{v}")?;
            }
            write!(f, ")")
        }
    }
}

// ------------------------------------------------------------------ parsing

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Attr(String),
    Op(&'static str),
    Dot(String),
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let s: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < s.len() {
        let c = s[i];
        if c.is_whitespace() || c == '\\' && s.get(i + 1) == Some(&'n') {
            i += if c == '\\' { 2 } else { 1 };
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && s.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let st = i;
            while i < s.len() && (s[i].is_ascii_digit() || s[i] == '.' || s[i] == 'e' || ((s[i] == '-' || s[i] == '+') && (s[i - 1] == 'e'))) {
                i += 1;
            }
            let t: String = s[st..i].iter().collect();
            out.push(Tok::Num(t.parse().map_err(|_| format!("bad number '{t}'"))?));
            continue;
        }
        if c == '.' {
            let st = i + 1;
            i += 1;
            while i < s.len() && s[i].is_ascii_alphabetic() {
                i += 1;
            }
            if i == st {
                return Err("a '.' with no swizzle after it".into());
            }
            out.push(Tok::Dot(s[st..i].iter().collect()));
            continue;
        }
        if c == '$' || c.is_ascii_alphabetic() || c == '_' {
            let st = i;
            i += 1;
            while i < s.len() && (s[i].is_ascii_alphanumeric() || s[i] == '_') {
                i += 1;
            }
            let t: String = s[st..i].iter().collect();
            out.push(if let Some(a) = t.strip_prefix('$') { Tok::Attr(a.to_string()) } else { Tok::Ident(t) });
            continue;
        }
        let two: String = s[i..(i + 2).min(s.len())].iter().collect();
        let op = match two.as_str() {
            "<=" => Some("<="),
            ">=" => Some(">="),
            "==" => Some("=="),
            "!=" => Some("!="),
            "&&" => Some("&&"),
            "||" => Some("||"),
            _ => None,
        };
        if let Some(o) = op {
            out.push(Tok::Op(o));
            i += 2;
            continue;
        }
        let one = match c {
            '+' => "+", '-' => "-", '*' => "*", '/' => "/", '(' => "(", ')' => ")", ',' => ",", '?' => "?", ':' => ":",
            '<' => "<", '>' => ">", '=' => "=", ';' => ";", '!' => "!",
            _ => return Err(format!("unexpected character '{c}'")),
        };
        out.push(Tok::Op(one));
        i += 1;
    }
    Ok(out)
}

#[derive(Debug, Clone)]
enum Ex {
    Num(f64),
    Attr(String),
    Local(String),
    Neg(Box<Ex>),
    Not(Box<Ex>),
    Bin(&'static str, Box<Ex>, Box<Ex>),
    Tern(Box<Ex>, Box<Ex>, Box<Ex>),
    Call(String, Vec<Ex>),
    Swizzle(Box<Ex>, String),
}

struct P {
    t: Vec<Tok>,
    i: usize,
    /// How deep the parser has recursed, against `MAX_DEPTH`.
    depth: usize,
}

/// The deepest expression read. Real DynamicParams are a handful of levels; a mod's `.vmat` decides what is in it, so
/// a deeper one (thousands of nested brackets, or a chain of thousands of operators) is refused instead of
/// overflowing the stack while it is parsed, analysed or dropped.
const MAX_DEPTH: usize = 100;

/// An expression's depth, never more than `MAX_DEPTH + 1` deep to compute: `P` refuses any node deeper than that.
fn depth(e: &Ex) -> usize {
    1 + match e {
        Ex::Num(_) | Ex::Attr(_) | Ex::Local(_) => 0,
        Ex::Neg(a) | Ex::Not(a) | Ex::Swizzle(a, _) => depth(a),
        Ex::Bin(_, a, b) => depth(a).max(depth(b)),
        Ex::Tern(a, b, c) => depth(a).max(depth(b)).max(depth(c)),
        Ex::Call(_, args) => args.iter().map(depth).max().unwrap_or(0),
    }
}

impl P {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn eat(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Op(o)) if *o == op) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn want(&mut self, op: &str) -> Result<(), String> {
        if self.eat(op) { Ok(()) } else { Err(format!("expected '{op}' at token {}", self.i)) }
    }
    /// `e`, unless it is deeper than `MAX_DEPTH`.
    fn bounded(&self, e: Ex) -> Result<Ex, String> {
        if depth(&e) > MAX_DEPTH {
            return Err(format!("expression nested deeper than {MAX_DEPTH} levels"));
        }
        Ok(e)
    }
    fn enter(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(format!("expression nested deeper than {MAX_DEPTH} levels"));
        }
        Ok(())
    }
    fn expr(&mut self) -> Result<Ex, String> {
        self.enter()?;
        let e = self.expr_inner();
        self.depth -= 1;
        e
    }
    fn expr_inner(&mut self) -> Result<Ex, String> {
        let c = self.or()?;
        if self.eat("?") {
            let a = self.expr()?;
            self.want(":")?;
            let b = self.expr()?;
            return self.bounded(Ex::Tern(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }
    fn bin_level(&mut self, ops: &[&'static str], next: fn(&mut P) -> Result<Ex, String>) -> Result<Ex, String> {
        let mut l = next(self)?;
        'outer: loop {
            for &o in ops {
                if self.eat(o) {
                    let r = next(self)?;
                    l = self.bounded(Ex::Bin(o, Box::new(l), Box::new(r)))?;
                    continue 'outer;
                }
            }
            return Ok(l);
        }
    }
    fn or(&mut self) -> Result<Ex, String> {
        self.bin_level(&["||"], P::and)
    }
    fn and(&mut self) -> Result<Ex, String> {
        self.bin_level(&["&&"], P::cmp)
    }
    fn cmp(&mut self) -> Result<Ex, String> {
        self.bin_level(&["<=", ">=", "==", "!=", "<", ">"], P::add)
    }
    fn add(&mut self) -> Result<Ex, String> {
        self.bin_level(&["+", "-"], P::mul)
    }
    fn mul(&mut self) -> Result<Ex, String> {
        self.bin_level(&["*", "/"], P::unary)
    }
    fn unary(&mut self) -> Result<Ex, String> {
        self.enter()?;
        let e = self.unary_inner();
        self.depth -= 1;
        e
    }
    fn unary_inner(&mut self) -> Result<Ex, String> {
        if self.eat("-") {
            let a = self.unary()?;
            return self.bounded(Ex::Neg(Box::new(a)));
        }
        if self.eat("!") {
            let a = self.unary()?;
            return self.bounded(Ex::Not(Box::new(a)));
        }
        if self.eat("+") {
            return self.unary();
        }
        self.postfix()
    }
    fn postfix(&mut self) -> Result<Ex, String> {
        let mut e = self.atom()?;
        while let Some(Tok::Dot(sw)) = self.peek().cloned() {
            self.i += 1;
            e = self.bounded(Ex::Swizzle(Box::new(e), sw))?;
        }
        Ok(e)
    }
    fn atom(&mut self) -> Result<Ex, String> {
        match self.peek().cloned() {
            Some(Tok::Num(n)) => {
                self.i += 1;
                Ok(Ex::Num(n))
            }
            Some(Tok::Attr(a)) => {
                self.i += 1;
                Ok(Ex::Attr(a))
            }
            Some(Tok::Ident(id)) => {
                self.i += 1;
                if self.eat("(") {
                    let mut args = Vec::new();
                    if !self.eat(")") {
                        loop {
                            args.push(self.expr()?);
                            if self.eat(")") { break; }
                            self.want(",")?;
                        }
                    }
                    self.bounded(Ex::Call(id, args))
                } else {
                    Ok(Ex::Local(id))
                }
            }
            Some(Tok::Op("(")) => {
                self.i += 1;
                let e = self.expr()?;
                self.want(")")?;
                Ok(e)
            }
            other => Err(format!("unexpected {other:?} at token {}", self.i)),
        }
    }
}

/// A parsed expression program: local assignments then a return.
pub struct Program {
    stmts: Vec<(String, Ex)>,
    ret: Ex,
}

pub fn parse(src: &str) -> Result<Program, String> {
    let toks = lex(src)?;
    let mut p = P { t: toks, i: 0, depth: 0 };
    let mut stmts = Vec::new();
    loop {
        match p.peek().cloned() {
            Some(Tok::Ident(id)) if id == "return" => {
                p.i += 1;
                let e = p.expr()?;
                p.eat(";");
                if p.peek().is_some() {
                    return Err("tokens after return".into());
                }
                return Ok(Program { stmts, ret: e });
            }
            Some(Tok::Ident(id)) => {
                p.i += 1;
                p.want("=")?;
                let e = p.expr()?;
                p.want(";")?;
                stmts.push((id, e));
            }
            None => return Err("missing return".into()),
            other => return Err(format!("expected a statement, got {other:?}")),
        }
    }
}

// ------------------------------------------------------------------ analysis

#[derive(Debug, Clone, PartialEq)]
pub enum Note {
    DivideByZero,
    UnknownAttr(String),
    Random,
    UsesTime,
    UnknownFunction(String),
}

pub struct Analysis {
    pub value: Val,
    pub cost: usize,
    pub notes: Vec<Note>,
    pub attrs: Vec<String>,
}

/// Known attribute ranges. Anything else is unbounded and reported, because a proof is only as good as
/// its inputs.
pub fn default_attrs() -> HashMap<String, Iv> {
    let mut m = HashMap::new();
    for (k, lo, hi) in [
        ("ent_gametime", 0.0, 10800.0), // a 3-hour match ceiling
        ("ent_age", 0.0, 10800.0),
        ("AmmoPercent", 0.0, 1.0),
        ("ReloadProgress", 0.0, 1.0),
        ("ent_invuln", 0.0, 1.0),
        ("ent_weakened", 0.0, 1.0),
        ("ent_weakpoint_hit", 0.0, 1.0),
    ] {
        m.insert(k.to_string(), Iv::new(lo, hi));
    }
    m
}

struct Cx<'a> {
    attrs: &'a HashMap<String, Iv>,
    locals: HashMap<String, Val>,
    cost: usize,
    notes: Vec<Note>,
    used: Vec<String>,
}

impl Cx<'_> {
    fn note(&mut self, n: Note) {
        if !self.notes.contains(&n) {
            self.notes.push(n);
        }
    }
    fn ev(&mut self, e: &Ex) -> Val {
        match e {
            Ex::Num(n) => Val::s(Iv::pt(*n)),
            Ex::Attr(a) => {
                if !self.used.contains(a) {
                    self.used.push(a.clone());
                }
                match self.attrs.get(a) {
                    Some(iv) => Val::s(*iv),
                    None => {
                        self.note(Note::UnknownAttr(a.clone()));
                        // Unknown width: model as a scalar. Swizzles broadcast it (`$luminance.x`),
                        // and it never widens a float3 result into a float4.
                        Val::s(Iv::ALL)
                    }
                }
            }
            Ex::Local(l) => self.locals.get(l).cloned().unwrap_or(Val::s(Iv::ALL)),
            Ex::Neg(x) => {
                self.cost += 1;
                self.ev(x).map(|i| Iv::new(-i.hi, -i.lo))
            }
            Ex::Not(x) => {
                self.cost += 1;
                let v = self.ev(x).range();
                Val::s(if v.lo > 0.0 || v.hi < 0.0 { Iv::pt(0.0) } else if v == Iv::pt(0.0) { Iv::pt(1.0) } else { Iv::new(0.0, 1.0) })
            }
            Ex::Bin(op, a, b) => {
                self.cost += 1;
                let (x, y) = (self.ev(a), self.ev(b));
                match *op {
                    "+" => Val::zip(&x, &y, |p, q| Iv { lo: p.lo + q.lo, hi: p.hi + q.hi }),
                    "-" => Val::zip(&x, &y, |p, q| Iv { lo: p.lo - q.hi, hi: p.hi - q.lo }),
                    "*" => Val::zip(&x, &y, mul),
                    "/" => {
                        if y.0.iter().any(|q| q.contains0()) {
                            self.note(Note::DivideByZero);
                        }
                        Val::zip(&x, &y, |p, q| {
                            if q.contains0() { Iv::ALL } else { mul(p, Iv::new(1.0 / q.lo, 1.0 / q.hi)) }
                        })
                    }
                    _ => {
                        let (p, q) = (x.range(), y.range());
                        let (t, f) = match *op {
                            "<" => (p.hi < q.lo, p.lo >= q.hi),
                            "<=" => (p.hi <= q.lo, p.lo > q.hi),
                            ">" => (p.lo > q.hi, p.hi <= q.lo),
                            ">=" => (p.lo >= q.hi, p.hi < q.lo),
                            "==" => (p.lo == p.hi && q.lo == q.hi && p.lo == q.lo, p.hi < q.lo || p.lo > q.hi),
                            "!=" => (p.hi < q.lo || p.lo > q.hi, p.lo == p.hi && q.lo == q.hi && p.lo == q.lo),
                            "&&" => (p.lo > 0.0 && q.lo > 0.0, p == Iv::pt(0.0) || q == Iv::pt(0.0)),
                            "||" => (p.lo > 0.0 || q.lo > 0.0, p == Iv::pt(0.0) && q == Iv::pt(0.0)),
                            _ => (false, false),
                        };
                        Val::s(if t { Iv::pt(1.0) } else if f { Iv::pt(0.0) } else { Iv::new(0.0, 1.0) })
                    }
                }
            }
            Ex::Tern(c, a, b) => {
                self.cost += 1;
                let cv = self.ev(c).range();
                let (x, y) = (self.ev(a), self.ev(b));
                if cv == Iv::pt(1.0) { x } else if cv == Iv::pt(0.0) { y } else { Val::zip(&x, &y, Iv::hull) }
            }
            Ex::Swizzle(x, sw) => {
                let v = self.ev(x);
                Val(sw.chars().map(|c| v.comp(match c { 'x' | 'r' => 0, 'y' | 'g' => 1, 'z' | 'b' => 2, _ => 3 })).collect())
            }
            Ex::Call(f, args) => {
                self.cost += 1;
                let a: Vec<Val> = args.iter().map(|x| self.ev(x)).collect();
                let arg = |i: usize| a.get(i).cloned().unwrap_or(Val::s(Iv::pt(0.0)));
                match f.as_str() {
                    "time" => {
                        self.note(Note::UsesTime);
                        Val::s(Iv::new(0.0, f64::INFINITY))
                    }
                    "random" => {
                        self.note(Note::Random);
                        let (lo, hi) = (arg(0).range(), arg(1).range());
                        Val::s(Iv::new(lo.lo.min(hi.lo), lo.hi.max(hi.hi)))
                    }
                    "sin" => arg(0).map(sin_iv),
                    "cos" => arg(0).map(|i| sin_iv(Iv { lo: i.lo + std::f64::consts::FRAC_PI_2, hi: i.hi + std::f64::consts::FRAC_PI_2 })),
                    "abs" => arg(0).map(|i| if i.contains0() { Iv::new(0.0, i.lo.abs().max(i.hi.abs())) } else { Iv::new(i.lo.abs(), i.hi.abs()) }),
                    "frac" => arg(0).map(|i| if i.bounded() && i.lo.floor() == i.hi.floor() { Iv::new(i.lo - i.lo.floor(), i.hi - i.hi.floor()) } else { Iv::new(0.0, 1.0) }),
                    "floor" => arg(0).map(|i| monotone(i, f64::floor)),
                    "ceil" => arg(0).map(|i| monotone(i, f64::ceil)),
                    "sqrt" => arg(0).map(|i| monotone(Iv::new(i.lo.max(0.0), i.hi.max(0.0)), f64::sqrt)),
                    "saturate" => arg(0).map(|i| Iv::new(i.lo.clamp(0.0, 1.0), i.hi.clamp(0.0, 1.0))),
                    "min" => Val::zip(&arg(0), &arg(1), |p, q| Iv { lo: p.lo.min(q.lo), hi: p.hi.min(q.hi) }),
                    "max" => Val::zip(&arg(0), &arg(1), |p, q| Iv { lo: p.lo.max(q.lo), hi: p.hi.max(q.hi) }),
                    "clamp" => {
                        let (x, lo, hi) = (arg(0), arg(1), arg(2));
                        let n = x.comps().max(lo.comps()).max(hi.comps());
                        Val((0..n)
                            .map(|k| {
                                let (v, l, h) = (x.comp(k), lo.comp(k), hi.comp(k));
                                Iv { lo: v.lo.max(l.lo).min(h.hi), hi: v.hi.min(h.hi).max(l.lo) }
                            })
                            .collect())
                    }
                    "lerp" => {
                        let (p, q, t) = (arg(0), arg(1), arg(2));
                        let n = p.comps().max(q.comps()).max(t.comps());
                        Val((0..n)
                            .map(|k| {
                                let (a0, b0, t0) = (p.comp(k), q.comp(k), t.comp(k));
                                // a + (b-a)t, evaluated over the corners
                                let mut vs = vec![];
                                for &x in &[a0.lo, a0.hi] {
                                    for &y in &[b0.lo, b0.hi] {
                                        for &s in &[t0.lo, t0.hi] {
                                            vs.push(x + (y - x) * s);
                                        }
                                    }
                                }
                                Iv::of(&vs)
                            })
                            .collect())
                    }
                    "smoothstep" => {
                        let (e0, e1, x) = (arg(0).range(), arg(1).range(), arg(2));
                        if e0.lo == e0.hi && e1.lo == e1.hi && e0.lo < e1.lo {
                            let ss = |v: f64| {
                                let t = ((v - e0.lo) / (e1.lo - e0.lo)).clamp(0.0, 1.0);
                                t * t * (3.0 - 2.0 * t)
                            };
                            x.map(|i| Iv::new(ss(i.lo), ss(i.hi)))
                        } else {
                            x.map(|_| Iv::new(0.0, 1.0))
                        }
                    }
                    "pow" => Val::zip(&arg(0), &arg(1), |b, e| {
                        if b.lo >= 0.0 && e.bounded() {
                            Iv::of(&[b.lo.powf(e.lo), b.lo.powf(e.hi), b.hi.powf(e.lo), b.hi.powf(e.hi)])
                        } else {
                            Iv::ALL
                        }
                    }),
                    "RemapVal" => {
                        // RemapVal(x, a, b, c, d): linear map [a,b] -> [c,d], unclamped
                        let (x, a0, b0, c0, d0) = (arg(0).range(), arg(1).range(), arg(2).range(), arg(3).range(), arg(4).range());
                        if b0.contains0() && a0.contains0() && (b0.hi - a0.lo) == 0.0 {
                            self.note(Note::DivideByZero);
                        }
                        let f = |v: f64| c0.lo + (d0.lo - c0.lo) * (v - a0.lo) / (b0.lo - a0.lo);
                        Val::s(Iv::new(f(x.lo), f(x.hi)))
                    }
                    "float2" | "float3" | "float4" => {
                        let mut comps = Vec::new();
                        for v in &a {
                            comps.extend(v.0.iter().copied());
                        }
                        let n = f.as_bytes()[5] as usize - b'0' as usize;
                        while comps.len() < n {
                            comps.push(*comps.last().unwrap_or(&Iv::pt(0.0)));
                        }
                        comps.truncate(n);
                        Val(comps)
                    }
                    other => {
                        self.note(Note::UnknownFunction(other.to_string()));
                        Val::s(Iv::ALL)
                    }
                }
            }
        }
    }
}

pub fn analyze(p: &Program, attrs: &HashMap<String, Iv>) -> Analysis {
    let mut cx = Cx { attrs, locals: HashMap::new(), cost: 0, notes: vec![], used: vec![] };
    for (name, e) in &p.stmts {
        let v = cx.ev(e);
        cx.locals.insert(name.clone(), v);
    }
    let value = cx.ev(&p.ret);
    Analysis { value, cost: cx.cost, notes: cx.notes, attrs: cx.used }
}

/// Pull every `"param" "expression"` pair out of a .vmat's DynamicParams block.
pub fn vmat_dynamic_params(vmat: &str) -> Vec<(String, String)> {
    let Some(start) = vmat.find("\"DynamicParams\"") else { return vec![] };
    let body = &vmat[start..];
    let Some(open) = body.find('{') else { return vec![] };
    let Some(close) = body[open..].find('}') else { return vec![] };
    let mut out = Vec::new();
    for line in body[open + 1..open + close].lines() {
        let q: Vec<&str> = line.split('"').collect();
        if q.len() >= 5 {
            out.push((q[1].to_string(), q[3].to_string()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(src: &str) -> Iv {
        analyze(&parse(src).unwrap(), &default_attrs()).value.range()
    }

    #[test]
    fn valves_inferno_glow_is_bounded() {
        // shipped on Infernus: 4*sin(2t)+5 -> [1, 9]
        assert_eq!(range("return 4*sin(2*time())+5;"), Iv::new(1.0, 9.0));
    }

    #[test]
    fn valves_artist_weapon_reads_ammo() {
        assert_eq!(range("return max($AmmoPercent,smoothstep(.5,1,$ReloadProgress));"), Iv::new(0.0, 1.0));
    }

    #[test]
    fn valves_flicker_with_random_is_clamped() {
        let a = analyze(&parse("return clamp(7.25+1.15*sin(time()*6.2832*.4)+random(-.1,.1),6,10);").unwrap(), &default_attrs());
        assert!(a.value.range().lo >= 6.0 && a.value.range().hi <= 10.0);
        assert!(a.notes.contains(&Note::Random) && a.notes.contains(&Note::UsesTime));
    }

    #[test]
    fn nurses_envelope_state_machine_parses_and_flags_unknown_inputs() {
        let src = "v0 = 0;\\nv1 = .1;\\nv2 = 0;\\nv3 = .5;\\nv4 = $ent_gametime-$UltCastStart;\\nv5 = (v4-(v2+$UltChannelDuration))/v3;\\nv6 = clamp((v4<$UltChannelDuration) ? (v4-v0)/v1 : 1-v5,0,1);\\nreturn lerp(float3(0,0,0),float3(1,1,1),v6);";
        let a = analyze(&parse(src).unwrap(), &default_attrs());
        assert_eq!(a.value.comps(), 3);
        assert_eq!(a.value.range(), Iv::new(0.0, 1.0), "the clamp proves the envelope stays in [0,1]");
        assert!(a.notes.iter().any(|n| matches!(n, Note::UnknownAttr(x) if x == "UltCastStart")));
    }

    #[test]
    fn divide_by_zero_is_caught() {
        let a = analyze(&parse("return 1/$AmmoPercent;").unwrap(), &default_attrs());
        assert!(a.notes.contains(&Note::DivideByZero));
        assert!(!a.value.range().bounded());
        let b = analyze(&parse("return 1/max(.001,$AmmoPercent);").unwrap(), &default_attrs());
        assert!(!b.notes.contains(&Note::DivideByZero));
        assert_eq!(b.value.range(), Iv::new(1.0, 1000.0));
    }

    #[test]
    fn frac_tick_and_sin_on_small_ranges() {
        assert_eq!(range("return frac(time());"), Iv::new(0.0, 1.0));
        let s = range("return sin(.5*$AmmoPercent);");
        assert!(s.lo >= 0.0 && s.hi < 0.48, "sin on [0,.5] is [0, sin .5]: {s}");
    }

    #[test]
    fn ternary_on_a_known_condition_picks_a_branch() {
        assert_eq!(range("return 2 > 1 ? 5 : 100;"), Iv::pt(5.0));
        assert_eq!(range("return $ent_invuln > 2 ? 5 : 0;"), Iv::pt(0.0));
    }

    #[test]
    fn reads_dynamic_params_from_a_vmat() {
        let vmat = "Layer0\n{\n\t\"DynamicParams\"\n\t{\n\t\t\"g_flSelfIllumScale1\"\t\"return 4*sin(2*time())+5;\"\n\t}\n}";
        assert_eq!(vmat_dynamic_params(vmat), vec![("g_flSelfIllumScale1".into(), "return 4*sin(2*time())+5;".into())]);
    }
}
