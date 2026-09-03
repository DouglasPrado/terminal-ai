//! End-to-end against a real `ai-memory` sidecar.
//!
//! `#[ignore]`d, so `cargo test` stays hermetic; run with `cargo test -- --ignored`. Every test
//! here uses an **ephemeral port and a temporary data directory** — never the shared store the app
//! uses in production, which belongs to the user and their own agents.
//!
//! These cover the things a mock cannot: that the CLI's real output parses, that a real server is
//! recognised by the probe, and that a cold start fits the budget SC-012 promises.

// clippy.toml's allow-expect-in-tests covers #[cfg(test)] items, but not the helper functions of
// an integration-test crate, which clippy does not recognise as test code. This is a test-only
// file: a panicking fixture is the correct behaviour here.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::{Duration, Instant};
use terminal_ai_domain::memory::KernelScope;
use terminal_ai_memory_kernel::cli::{KernelCli, KernelConfig};
use terminal_ai_memory_kernel::probe::{probe, ProbeOutcome};

/// The binary the app would ship, if it has been fetched.
fn sidecar() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?
        .join("src-tauri")
        .join("binaries");
    std::fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("ai-memory"))
        })
}

/// A port nothing is listening on. Bind, read the port, drop the listener.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.local_addr().expect("addr").port()
}

struct Sidecar {
    child: tokio::process::Child,
    cli: KernelCli,
    _dir: tempfile::TempDir,
    server_url: String,
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        // A test that leaves a server listening would poison the next one.
        let _ = self.child.start_kill();
    }
}

async fn start() -> Option<(Sidecar, Duration)> {
    let binary = sidecar()?;
    let dir = tempfile::tempdir().expect("temp dir");
    let port = free_port();
    let server_url = format!("http://127.0.0.1:{port}");

    let config = KernelConfig {
        binary,
        server_url: server_url.clone(),
        bind: format!("127.0.0.1:{port}"),
        // Never the shared store: these tests write test data.
        data_dir: Some(dir.path().to_path_buf()),
        token: None,
        // Off, so the test does not pull an 87 MB model down a CI link.
        hybrid_search: false,
    };
    let cli = KernelCli::new(config);

    let started = Instant::now();
    let mut command = cli.serve_command();
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let child = command.spawn().expect("spawn sidecar");

    for _ in 0..150 {
        if probe(&server_url, None).await == ProbeOutcome::Kernel {
            let elapsed = started.elapsed();
            return Some((
                Sidecar {
                    child,
                    cli,
                    _dir: dir,
                    server_url,
                },
                elapsed,
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the sidecar never became ready");
}

fn scope() -> KernelScope {
    KernelScope {
        workspace: "default".into(),
        project: "integration-test".into(),
        path_prefix: "terminal-ai/project".into(),
    }
}

#[tokio::test]
#[ignore = "needs the ai-memory sidecar; run with --ignored"]
async fn a_cold_start_fits_the_budget() {
    let Some((sidecar, elapsed)) = start().await else {
        eprintln!("no sidecar binary; run scripts/fetch-ai-memory.sh");
        return;
    };
    // SC-012 allows 15s for a genuinely cold start — a fresh data directory, which is what this is.
    assert!(
        elapsed < Duration::from_secs(15),
        "cold start took {elapsed:?}, over the 15s budget"
    );

    // And a warm probe against the now-running server is effectively instant, which is what the
    // 5s typical-start figure rests on.
    let warm = Instant::now();
    assert_eq!(probe(&sidecar.server_url, None).await, ProbeOutcome::Kernel);
    assert!(warm.elapsed() < Duration::from_secs(1));
}

#[tokio::test]
#[ignore = "needs the ai-memory sidecar; run with --ignored"]
async fn write_search_read_delete_round_trip() {
    let Some((sidecar, _)) = start().await else {
        eprintln!("no sidecar binary; run scripts/fetch-ai-memory.sh");
        return;
    };
    let scope = scope();
    let path = "terminal-ai/project/fact/round-trip-0123abcd.md";

    sidecar
        .cli
        .write_page(
            &scope,
            path,
            "Round trip",
            "fact",
            "# Round trip\n\nUNIQUEMARKER body.\n",
        )
        .await
        .expect("write");

    let status = sidecar.cli.status().await.expect("status");
    assert_eq!(
        status.version,
        terminal_ai_memory_kernel::cli::PINNED_VERSION
    );
    assert!(status.counts.is_some_and(|c| c.pages_latest >= 1));

    let client = terminal_ai_memory_kernel::http::ReadClient::new(&sidecar.server_url, None)
        .expect("read client");
    let hits = client
        .search(&scope, "UNIQUEMARKER", 10)
        .await
        .expect("search");
    assert!(
        hits.iter().any(|hit| hit.path == path),
        "written page must be findable"
    );

    let page = client.read_page(&scope, path).await.expect("read");
    assert!(page.body_markdown.contains("UNIQUEMARKER"));

    sidecar.cli.delete_page(&scope, path).await.expect("delete");
    let after = client
        .search(&scope, "UNIQUEMARKER", 10)
        .await
        .expect("search again");
    assert!(
        !after.iter().any(|hit| hit.path == path),
        "deleted page must be gone"
    );
}

#[tokio::test]
#[ignore = "needs the ai-memory sidecar; run with --ignored"]
async fn a_scoped_search_never_crosses_projects() {
    // SC-014 against a real server, not a mock: the same word in two projects, and a scoped query
    // that returns only one of them. This is the guarantee the whole scope mapper exists to make.
    let Some((sidecar, _)) = start().await else {
        eprintln!("no sidecar binary; run scripts/fetch-ai-memory.sh");
        return;
    };
    let alpha = KernelScope {
        project: "alpha".into(),
        ..scope()
    };
    let beta = KernelScope {
        project: "beta".into(),
        ..scope()
    };

    for (scope, body) in [
        (&alpha, "SHAREDWORD in alpha"),
        (&beta, "SHAREDWORD in beta"),
    ] {
        sidecar
            .cli
            .write_page(
                scope,
                "terminal-ai/project/fact/secret-0123abcd.md",
                "Secret",
                "fact",
                &format!("# Secret\n\n{body}\n"),
            )
            .await
            .expect("write");
    }

    let client = terminal_ai_memory_kernel::http::ReadClient::new(&sidecar.server_url, None)
        .expect("read client");
    let hits = client
        .search(&alpha, "SHAREDWORD", 20)
        .await
        .expect("search");

    assert!(!hits.is_empty(), "alpha should find its own page");
    assert!(
        hits.iter().all(|hit| hit.project == "alpha"),
        "a scoped search leaked into another project: {:?}",
        hits.iter().map(|h| &h.project).collect::<Vec<_>>()
    );
}
