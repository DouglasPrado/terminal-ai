//! The bundle declaration is load-bearing, and nothing else checks it.
//!
//! `resolve_hooks_dir` in `src/memory.rs` looks for the kernel's hook bundle in
//! `Contents/Resources/hooks`, and the wiring flow passes it to the kernel as `--hooks-dir`.
//! Whether anything actually lands there is decided entirely by `tauri.conf.json` — not by any
//! code path a test would otherwise exercise. It shipped wrong once: the resolver, the CLI flag
//! and the fetch script were all in place while the bundler was never told to copy the directory,
//! so every release built fine and then failed at runtime with "could not locate hooks directory".

// A test that cannot read its own config has nothing to assert; panicking is the correct
// outcome. Same rationale as crates/memory-kernel/tests/sidecar.rs.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

fn bundle_config() -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    let text = std::fs::read_to_string(&path).expect("tauri.conf.json is readable");
    let config: serde_json::Value = serde_json::from_str(&text).expect("tauri.conf.json is JSON");
    config["bundle"].clone()
}

#[test]
fn the_hook_bundle_is_copied_to_the_resources_root() {
    let resources = &bundle_config()["resources"];
    let map = resources.as_object().unwrap_or_else(|| {
        panic!(
            "`resources` must be a source→target map: the list form preserves the source path, \
             which would put the hooks at Resources/binaries/hooks, where nothing looks for them. \
             Got: {resources}"
        )
    });

    let target = map.get("binaries/hooks/").unwrap_or_else(|| {
        panic!("the hook bundle is not declared, so no release will contain it. Got: {resources}")
    });

    assert_eq!(
        target.as_str(),
        Some("hooks/"),
        "hooks must land at Resources/hooks/, which is what src/memory.rs::resolve_hooks_dir reads"
    );
}

#[test]
fn the_sidecar_licence_is_still_shipped() {
    // Bundling a third-party binary obliges us to carry its licence; it travelled with the hooks
    // change and has no other guard.
    let resources = &bundle_config()["resources"];
    let map = resources.as_object().expect("resources is a map");
    assert!(
        map.contains_key("resources/third-party/ai-memory-LICENSE.txt"),
        "the ai-memory licence must ship with the binary it covers. Got: {resources}"
    );
}
