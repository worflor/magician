//! The commit the binary is built from, for `magician --version` and the `toolchain` field of a grade record:
//! `git rev-parse --short=12 HEAD`, with `-dirty` when tracked files differ from it, or `unknown` outside a checkout.

use std::path::Path;
use std::process::Command;

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let o = Command::new("git").arg("-C").arg(root).args(args).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    for p in [".git/HEAD", ".git/index", ".git/refs", ".git/packed-refs", "crates"] {
        println!("cargo:rerun-if-changed={}", root.join(p).display());
    }
    let sha = git(&root, &["rev-parse", "--short=12", "HEAD"]);
    let dirty = git(&root, &["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
    let text = match sha {
        Some(s) if dirty => format!("{s}-dirty"),
        Some(s) => s,
        None => "unknown".to_string(),
    };
    println!("cargo:rustc-env=MAGICIAN_GIT={text}");
}
