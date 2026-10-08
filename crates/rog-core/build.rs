use std::path::Path;
use std::process::Command;

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn main() {
    println!("cargo:rerun-if-env-changed=ROG_HELPER_BUILD_COMMIT");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let checkout = git(&root, &["rev-parse", "--show-toplevel"])
        .and_then(|path| Path::new(&path).canonicalize().ok())
        == root.canonicalize().ok();
    let commit = std::env::var("ROG_HELPER_BUILD_COMMIT").ok().or_else(|| {
        if !checkout {
            return None;
        }
        for target in [
            "HEAD".to_string(),
            git(&root, &["symbolic-ref", "-q", "HEAD"]).unwrap_or_default(),
        ] {
            if !target.is_empty() {
                if let Some(path) = git(&root, &["rev-parse", "--git-path", &target]) {
                    println!("cargo:rerun-if-changed={}", root.join(path).display());
                }
            }
        }
        git(&root, &["rev-parse", "HEAD"])
    });
    let commit = commit
        .filter(|value| value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .unwrap_or_else(|| "unknown (source archive without build provenance)".into());
    println!("cargo:rustc-env=ROG_HELPER_BUILD_COMMIT={commit}");
}
