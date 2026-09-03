mod commands;
pub mod events;
mod host;
mod state;

use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Boot-time fail-fast: if the app cannot create its directories, initialize logging, or open
    // its database, there is nothing to run — crashing with a clear message is the correct
    // behavior, so `clippy::expect_used` is allowed at these specific sites only.
    #[allow(clippy::expect_used)]
    let paths = terminal_ai_platform_macos::AppPaths::bootstrap()
        .expect("failed to create app directories");
    #[allow(clippy::expect_used)]
    let _log_guard =
        terminal_ai_platform_macos::init_logging(&paths).expect("failed to initialize logging");
    #[allow(clippy::expect_used)]
    let database =
        terminal_ai_persistence::Database::open(paths.database).expect("failed to open database");
    let (exit_tx, mut exit_rx) = tokio::sync::mpsc::unbounded_channel::<host::SessionExit>();
    let exit_database = database.clone();
    let usage_database = database.clone();
    // Boot-time fail-fast: a failure to start the Tauri event loop is unrecoverable.
    #[allow(clippy::expect_used)]
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::new(
            database,
            paths.cache.clone(),
            paths.skills.clone(),
            paths.memory.clone(),
            exit_tx,
        ))
        .setup(move |app| {
            let handle = app.handle().clone();
            // T058: when a PTY exits on its own, notify the UI and finalize its DB row so
            // `list_sessions` and activity indicators stop reporting dead sessions as running.
            tauri::async_runtime::spawn(async move {
                while let Some(exit) = exit_rx.recv().await {
                    let _ = handle.emit(
                        "process-exited",
                        events::ProcessExited {
                            session_id: exit.id.clone(),
                            exit_code: exit.exit_code,
                        },
                    );
                    let database = exit_database.clone();
                    let id = exit.id.clone();
                    let code = exit.exit_code;
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        let _ = terminal_ai_persistence::dao::SessionsDao(&database)
                            .finish(&id, "exited", code);
                    })
                    .await;
                }
            });
            // Autonomous usage poller (T076 + live push): refresh on an interval, persist the
            // snapshot to the DB, and emit `usage-updated` so the sidebar reflects fresh numbers
            // without the UI re-requesting. Per-provider cadence + the 300s floor live in refresh.
            let usage = app.state::<state::AppState>().usage.clone();
            let usage_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    if let Ok((result, snapshot)) = usage.refresh(None, false).await {
                        if result.scheduled {
                            let fetched_at = snapshot.updated_at.to_rfc3339();
                            let records: Vec<terminal_ai_persistence::dao::UsageSnapshotRecord> =
                                snapshot
                                    .providers
                                    .iter()
                                    .filter_map(|(id, card)| {
                                        serde_json::to_value(card).ok().map(|value| {
                                            terminal_ai_persistence::dao::UsageSnapshotRecord {
                                                provider_id: id.clone(),
                                                snapshot: value,
                                                fetched_at: fetched_at.clone(),
                                                stale: card.stale,
                                            }
                                        })
                                    })
                                    .collect();
                            let database = usage_database.clone();
                            let _ = tauri::async_runtime::spawn_blocking(move || {
                                let dao =
                                    terminal_ai_persistence::dao::UsageSnapshotsDao(&database);
                                for record in records {
                                    let _ = dao.upsert(&record);
                                }
                            })
                            .await;
                            let _ = usage_handle.emit("usage-updated", &snapshot);
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::resolve_env,
            commands::create_session,
            commands::write_input,
            commands::resize_session,
            commands::send_signal,
            commands::close_session,
            commands::restart_session,
            commands::list_sessions,
            commands::get_scrollback,
            commands::get_session_history,
            commands::resume_session,
            commands::list_workspaces,
            commands::create_workspace,
            commands::close_workspace,
            commands::save_layout,
            commands::load_layout,
            commands::list_providers,
            commands::detect_provider,
            commands::upsert_provider_profile,
            commands::list_projects,
            commands::set_project_archived,
            commands::set_project_name,
            commands::pick_directory,
            commands::set_workspace_root,
            commands::rename_workspace,
            commands::add_project_folder,
            commands::clone_project,
            commands::remove_project,
            commands::get_git_status,
            commands::get_usage,
            commands::refresh_usage,
            commands::create_worktree,
            commands::list_worktrees,
            commands::remove_worktree,
            commands::list_presets,
            commands::save_preset,
            commands::create_workspace_from_preset,
            commands::list_skills,
            commands::preview_skill_apply,
            commands::apply_skill,
            commands::remove_skill,
            commands::set_skill_binding,
            commands::delete_skill,
            commands::list_memory,
            commands::search_memory,
            commands::add_memory,
            commands::capture_selection_to_memory,
            commands::preview_memory_context,
            commands::get_settings,
            commands::set_settings,
            commands::notify
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Terminal AI");
}
