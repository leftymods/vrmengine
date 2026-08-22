use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=VRM_VIEWER_BUILD_HASH={hash}");

    // Build timestamp baked into the binary so the egui overlay can show
    // when the binary itself was built (matches the previous mtime approach
    // but without the libc runtime call). Kept as Unix seconds -> formatted
    // at display time.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    println!("cargo:rustc-env=VRM_VIEWER_BUILD_TIME={secs}");

    println!("cargo:rerun-if-changed=build.rs");
}
