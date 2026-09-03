use portable_pty::PtySize;
use std::{path::PathBuf, sync::mpsc, time::Duration};
use terminal_ai_domain::host::CommandSpec;
use terminal_ai_pty_runtime::PtyProcess;

#[test]
fn shell_is_interactive_and_resizable() {
    let (tx, rx) = mpsc::channel();
    let spec = CommandSpec {
        program: PathBuf::from("/bin/sh"),
        args: vec!["-i".into()],
        cwd: std::env::temp_dir(),
        env: vec![("TERM".into(), "xterm-256color".into())],
    };
    let process = PtyProcess::spawn(
        &spec,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
        move |bytes| {
            let _ = tx.send(bytes);
        },
        |_code| {},
    )
    .expect("spawn");
    process.resize(100, 30).expect("resize");
    process.write(b"printf 'PTY_OK\\n'\r").expect("write");
    let mut output = Vec::new();
    for _ in 0..10 {
        if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(250)) {
            output.extend(chunk);
            if String::from_utf8_lossy(&output).contains("PTY_OK") {
                break;
            }
        }
    }
    assert!(String::from_utf8_lossy(&output).contains("PTY_OK"));
    let _ = process.close();
}

#[test]
fn process_self_exit_is_detected() {
    let (tx, rx) = mpsc::channel();
    let spec = CommandSpec {
        program: PathBuf::from("/bin/sh"),
        args: vec!["-c".into(), "exit 3".into()],
        cwd: std::env::temp_dir(),
        env: vec![],
    };
    let _process = PtyProcess::spawn(
        &spec,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
        |_bytes| {},
        move |code| {
            let _ = tx.send(code);
        },
    )
    .expect("spawn");
    let code = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("exit callback fires");
    assert_eq!(code, Some(3));
}

#[test]
fn twelve_terminals_batch_heavy_output_without_starvation() {
    let started = std::time::Instant::now();
    let mut terminals = Vec::new();
    for index in 0..12 {
        let (tx, rx) = mpsc::channel();
        let script = format!(
            "i=0; while [ $i -lt 2000 ]; do printf 'terminal-{index}-%04d\\n' $i; i=$((i+1)); done; echo LOAD_DONE_{index}"
        );
        let spec = CommandSpec {
            program: PathBuf::from("/bin/sh"),
            args: vec!["-c".into(), script],
            cwd: std::env::temp_dir(),
            env: vec![("TERM".into(), "xterm-256color".into())],
        };
        let process = PtyProcess::spawn(
            &spec,
            PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            },
            move |bytes| {
                let _ = tx.send(bytes);
            },
            |_code| {},
        )
        .expect("spawn load terminal");
        terminals.push((index, process, rx));
    }
    for (index, process, rx) in terminals {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let expected = format!("LOAD_DONE_{index}");
        let mut output = Vec::new();
        while std::time::Instant::now() < deadline {
            if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) {
                output.extend(chunk);
                if String::from_utf8_lossy(&output).contains(&expected) {
                    break;
                }
            }
        }
        assert!(
            String::from_utf8_lossy(&output).contains(&expected),
            "terminal {index} was starved"
        );
        let _ = process.close();
    }
    assert!(started.elapsed() < Duration::from_secs(15));
}
