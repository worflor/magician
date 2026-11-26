//! The commit the binary is built from, for `magician --version`, the `toolchain` field of a grade record and
//! `magician update`: `git rev-parse --short=12 HEAD`, with `-dirty` when tracked files differ from it, or `unknown`
//! outside a checkout; and that commit's committer time (Unix seconds) and date (`YYYY-MM-DD`). The time is how
//! `update --check` orders two commits of one version, two made the same day included; the date is for people, and
//! orders a release whose `VERSION` was written before it carried a time.

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
    let date = git(&root, &["show", "-s", "--format=%cs", "HEAD"]).filter(|d| d.len() == 10).unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=MAGICIAN_GIT_DATE={date}");
    let time = git(&root, &["show", "-s", "--format=%ct", "HEAD"]).filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit())).unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=MAGICIAN_GIT_TIME={time}");
}
