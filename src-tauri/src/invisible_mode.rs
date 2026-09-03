//! Turns the invisible-mode decision into the three macOS switches.
//!
//! Thin on purpose: every rule that can be decided without a window handle lives in
//! `terminal_ai_domain::invisible_mode`. What is here is the part that cannot leave the
//! composition root, because an `AppHandle` and a window only exist here.

use crate::commands::AppError;
use crate::state::AppState;
use std::time::Instant;
use tauri::{AppHandle, Manager, WebviewWindow};

const APPLY_FAILED: &str = "INVISIBLE_MODE_APPLY_FAILED";

fn apply_failed(what: &str, error: impl std::fmt::Display) -> AppError {
    AppError {
        code: APPLY_FAILED.into(),
        message: format!("could not {what}: {error}"),
    }
}

/// Exclude (or re-include) one window from screen capture.
///
/// Also used on page load, so a window created after the mode was turned on is protected too
/// (FR-014): Tauri 2 exposes no window-created event, and a new window always loads a page.
pub fn protect_window(window: &WebviewWindow, enabled: bool) -> Result<(), AppError> {
    window
        .set_content_protected(enabled)
        .map_err(|error| apply_failed("exclude the window from screen capture", error))
}

/// Apply the mode, or undo it.
///
/// Turning **on** is all-or-nothing: if any switch refuses, everything that already applied is
/// undone and the error is returned, so the caller never persists a half-applied mode as active.
/// Turning **off** is best-effort in the reverse order — every switch is attempted even after one
/// fails, because stopping halfway through a restore leaves the user more hidden, not less.
pub async fn apply(app: &AppHandle, enabled: bool) -> Result<(), AppError> {
    let windows: Vec<WebviewWindow> = app.webview_windows().into_values().collect();
    if enabled {
        turn_on(app, &windows).await
    } else {
        turn_off(app, &windows)
    }
}

async fn turn_on(app: &AppHandle, windows: &[WebviewWindow]) -> Result<(), AppError> {
    let mut protected: Vec<&WebviewWindow> = Vec::new();
    for window in windows {
        match protect_window(window, true) {
            Ok(()) => protected.push(window),
            Err(error) => {
                for done in protected {
                    let _ = protect_window(done, false);
                }
                tracing::warn!(%error.message, "invisible mode: rolled back, capture protection failed");
                return Err(error);
            }
        }
    }

    // tao silently drops a dock hide that lands within a second of a dock show, to avoid leaving
    // duplicate Dock icons behind. Wait it out rather than report a hide that never happened.
    let wait = {
        let state = app.state::<AppState>();
        // Scoped so the guard is dropped before the await — a MutexGuard is not Send.
        #[allow(
            clippy::unwrap_used,
            reason = "poisoned only if a holder panicked; none can"
        )]
        let cooldown = state
            .dock_cooldown
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        cooldown.remaining(Instant::now())
    };
    if !wait.is_zero() {
        tokio::time::sleep(wait).await;
    }

    if let Err(error) = app.set_dock_visibility(false) {
        for window in windows {
            let _ = protect_window(window, false);
        }
        let error = apply_failed("remove the app from the Dock", error);
        tracing::warn!(%error.message, "invisible mode: rolled back, dock hide failed");
        return Err(error);
    }
    {
        let state = app.state::<AppState>();
        let mut cooldown = state
            .dock_cooldown
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cooldown.record_hide();
    }

    // The process-type transition can drop keyboard focus; give it back.
    if let Some(main) = app.webview_windows().values().next() {
        let _ = main.set_focus();
    }
    tracing::info!("invisible mode: on");
    Ok(())
}

fn turn_off(app: &AppHandle, windows: &[WebviewWindow]) -> Result<(), AppError> {
    let mut first_error: Option<AppError> = None;

    if let Err(error) = app.set_dock_visibility(true) {
        first_error = Some(apply_failed("restore the app to the Dock", error));
    } else {
        let state = app.state::<AppState>();
        let mut cooldown = state
            .dock_cooldown
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cooldown.record_show(Instant::now());
    }

    for window in windows {
        if let Err(error) = protect_window(window, false) {
            first_error.get_or_insert(error);
        }
    }

    match first_error {
        None => {
            tracing::info!("invisible mode: off");
            Ok(())
        }
        Some(error) => {
            tracing::warn!(%error.message, "invisible mode: restore incomplete");
            Err(error)
        }
    }
}
