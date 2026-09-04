//! Native PTY runtime.
#![forbid(unsafe_code)]

use nix::{
    sys::signal::{killpg, Signal as NixSignal},
    unistd::Pid,
};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use terminal_ai_domain::host::{CommandSpec, Signal};

pub struct PtyProcess {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    finished: Arc<AtomicBool>,
    pid: u32,
}

impl PtyProcess {
    pub fn spawn(
        spec: &CommandSpec,
        size: PtySize,
        on_output: impl Fn(Vec<u8>) + Send + 'static,
        on_exit: impl FnOnce(Option<i32>) + Send + 'static,
    ) -> Result<Self, PtyError> {
        let pair = native_pty_system()
            .openpty(size)
            .map_err(PtyError::backend)?;
        let mut command = CommandBuilder::new(&spec.program);
        command.args(&spec.args);
        command.cwd(&spec.cwd);
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(PtyError::backend)?;
        let pid = child.process_id().unwrap_or_default();
        let writer = pair.master.take_writer().map_err(PtyError::backend)?;
        let mut reader = pair.master.try_clone_reader().map_err(PtyError::backend)?;
        let (chunks_tx, chunks_rx) = mpsc::channel();
        thread::Builder::new()
            .name(format!("pty-reader-{pid}"))
            .spawn(move || read_output(&mut reader, chunks_tx))?;
        thread::Builder::new()
            .name(format!("pty-batcher-{pid}"))
            .spawn(move || batch_output(chunks_rx, on_output))?;
        drop(pair.slave);
        let child = Arc::new(Mutex::new(child));
        let finished = Arc::new(AtomicBool::new(false));
        let child_wait = Arc::clone(&child);
        let finished_wait = Arc::clone(&finished);
        thread::Builder::new()
            .name(format!("pty-waiter-{pid}"))
            .spawn(move || wait_for_exit(&child_wait, &finished_wait, on_exit))?;
        Ok(Self {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            child,
            finished,
            pid,
        })
    }
    pub const fn pid(&self) -> u32 {
        self.pid
    }
    pub fn write(&self, data: &[u8]) -> Result<(), PtyError> {
        let mut writer = self.writer.lock().map_err(|_| PtyError::Poisoned)?;
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
        if cols == 0 || rows == 0 {
            return Err(PtyError::InvalidSize);
        }
        self.master
            .lock()
            .map_err(|_| PtyError::Poisoned)?
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(PtyError::backend)
    }
    pub fn signal(&self, signal: Signal) -> Result<(), PtyError> {
        let master = self.master.lock().map_err(|_| PtyError::Poisoned)?;
        #[cfg(unix)]
        if let Some(group) = master.process_group_leader() {
            let signal = match signal {
                Signal::Int => NixSignal::SIGINT,
                Signal::Term => NixSignal::SIGTERM,
                Signal::Kill => NixSignal::SIGKILL,
                Signal::Hup => NixSignal::SIGHUP,
            };
            killpg(Pid::from_raw(group), signal)?;
            return Ok(());
        }
        drop(master);
        self.child.lock().map_err(|_| PtyError::Poisoned)?.kill()?;
        Ok(())
    }
    pub fn close(&self) -> Result<Option<i32>, PtyError> {
        // Signal the waiter thread to stop so it does not also fire `on_exit` for this session.
        self.finished.store(true, Ordering::SeqCst);
        let mut child = self.child.lock().map_err(|_| PtyError::Poisoned)?;
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status.exit_code() as i32));
        }
        child.kill()?;
        let status = child.wait()?;
        Ok(Some(status.exit_code() as i32))
    }
    pub fn try_exit(&self) -> Result<Option<i32>, PtyError> {
        Ok(self
            .child
            .lock()
            .map_err(|_| PtyError::Poisoned)?
            .try_wait()?
            .map(|status| status.exit_code() as i32))
    }
}

fn read_output(reader: &mut dyn Read, output: mpsc::Sender<Vec<u8>>) {
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                if output.send(buffer[..count].to_vec()).is_err() {
                    break;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
}

fn batch_output(input: mpsc::Receiver<Vec<u8>>, on_output: impl Fn(Vec<u8>)) {
    const WINDOW: Duration = Duration::from_millis(8);
    const MAX: usize = 64 * 1024;
    while let Ok(first) = input.recv() {
        let started = Instant::now();
        let mut batch = first;
        while batch.len() < MAX {
            let remaining = WINDOW.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                break;
            }
            match input.recv_timeout(remaining) {
                Ok(chunk) => batch.extend(chunk),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        on_output(batch)
    }
}

/// Polls the child until it exits on its own (or errors), then fires `on_exit` exactly once.
/// `finished` lets an explicit `close()` claim the reap first so `on_exit` is not double-fired.
fn wait_for_exit(
    child: &Mutex<Box<dyn Child + Send + Sync>>,
    finished: &AtomicBool,
    on_exit: impl FnOnce(Option<i32>),
) {
    loop {
        if finished.load(Ordering::SeqCst) {
            return;
        }
        let status = match child.lock() {
            Ok(mut guard) => guard.try_wait(),
            Err(_) => return,
        };
        match status {
            Ok(Some(status)) => {
                if !finished.swap(true, Ordering::SeqCst) {
                    on_exit(Some(status.exit_code() as i32));
                }
                return;
            }
            Ok(None) => thread::sleep(Duration::from_millis(150)),
            Err(_) => {
                if !finished.swap(true, Ordering::SeqCst) {
                    on_exit(None);
                }
                return;
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PtyError {
    #[error("PTY backend error: {0}")]
    Backend(String),
    #[error("PTY lock poisoned")]
    Poisoned,
    #[error("terminal size must be positive")]
    InvalidSize,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Signal(#[from] nix::Error),
}
impl PtyError {
    fn backend(error: impl std::fmt::Display) -> Self {
        Self::Backend(error.to_string())
    }
}
