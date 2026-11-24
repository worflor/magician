//! The DynamicParams expression parser and its interval proof on hostile text: an expression a mod ships must parse
//! or be refused, and its analysis must finish, never panic or recurse without bound. See `crates/fuzz`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that cannot unwrap has failed

#[global_allocator]
static CAP: fuzz::Capped = fuzz::Capped;

#[test]
fn expressions() {
    let seeds = [
        "return clamp(7.25+1.15*sin(time()*6.2832*.4)+random(-.1,.1),6,10);",
        "return 1/max(.001,$AmmoPercent);",
        "float x = $Health * 2; float y = x > 1 ? 1 : x; return lerp(y, 0, 3) + frac(time()) % 2;",
        "return ((((($A))))) - -1 / exists($B) && !($C <= 3) || abs(-2) ^ pow(2, 10);",
    ];
    let vmat = "Layer0\n{\n\tshader \"hero.vfx\"\n\t\"DynamicParams\"\n\t{\n\t\t\"g_flSelfIllumScale\" \"return 1/max(.001,$AmmoPercent);\"\n\t}\n}\n";
    let mut all: Vec<&str> = seeds.to_vec();
    all.push(vmat);
    fuzz::run_text("expr", fuzz::DEFAULT_CASES * 4, &all, |t| {
        if let Ok(p) = expr::parse(t) {
            let _ = expr::analyze(&p, &expr::default_attrs());
        }
        for (_, e) in expr::vmat_dynamic_params(t) {
            if let Ok(p) = expr::parse(&e) {
                let _ = expr::analyze(&p, &expr::default_attrs());
            }
        }
    });
}

#[test]
fn deep_nesting_is_refused_not_a_stack_overflow() {
    for n in [101usize, 1000, 100_000] {
        for src in [format!("return {}1{};", "(".repeat(n), ")".repeat(n)), format!("return {}1;", "-".repeat(n)), format!("return 1{};", "+1".repeat(n)), format!("return {}1{};", "abs(".repeat(n), ")".repeat(n))] {
            let e = expr::parse(&src).err().unwrap_or_else(|| panic!("{n} levels read"));
            assert!(e.contains("nested deeper than 100 levels"), "{e}");
        }
    }
    // what real materials do still reads
    assert!(expr::parse(&format!("return {}1{};", "(".repeat(40), ")".repeat(40))).is_ok());
    assert!(expr::parse(&format!("return 1{};", "+1".repeat(90))).is_ok());
}

#[test]
fn locals_that_build_on_each_other_are_analysed_in_linear_time() {
    // each local doubles the one before: inlining them would be 2^60 nodes
    let mut src = String::from("x0 = $Health;
");
    for i in 1..60 {
        src.push_str(&format!("x{i} = x{} + x{};
", i - 1, i - 1));
    }
    src.push_str("return x59;");
    let t = std::time::Instant::now();
    let p = expr::parse(&src).unwrap();
    let _ = expr::analyze(&p, &expr::default_attrs());
    assert!(t.elapsed().as_secs() < 2, "{:?}", t.elapsed());
}

#[test]
fn a_dot_with_no_swizzle_is_refused() {
    // found by the fuzzer: `$A.` analysed as a value with no components
    for src in ["return 1/max(.001,$A../mmoPercent);", "return $A.;", "return ($A).+1;"] {
        assert!(expr::parse(src).err().is_some_and(|e| e.contains("no swizzle")), "{src}");
    }
    let p = expr::parse("return $Health.x + $A.xy;").unwrap();
    let _ = expr::analyze(&p, &expr::default_attrs());
}
