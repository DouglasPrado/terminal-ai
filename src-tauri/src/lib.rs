mod commands;
pub mod events;
mod host;
mod invisible_mode;
mod memory;
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
    // The memory kernel: resolve a binary, build the supervisor, and hand it to the app state.
    // `None` for the config is a first-class outcome — the app runs fine without a kernel, it just
    // cannot show memory (Constitution VI).
    let kernel_settings = memory::KernelSettings::load(&database);
    let kernel_config = memory::build_config(&kernel_settings, None);
    let kernel_auto_start = kernel_settings.auto_start && kernel_config.is_some();
    let kernel = std::sync::Arc::new(terminal_ai_memory_kernel::runtime::Supervisor::new(
        terminal_ai_memory_kernel::runtime::SupervisorOptions {
            config: kernel_config,
            runtime_dir: terminal_ai_memory_kernel::runtime::runtime_dir(&paths.root),
            pending_migration: terminal_ai_persistence::dao::MemoryMigrationDao(&database)
                .pending_count()
                .unwrap_or(0),
        },
    ));
    let kernel_for_state = std::sync::Arc::clone(&kernel);
    let kernel_for_setup = std::sync::Arc::clone(&kernel);
    let kernel_for_exit = std::sync::Arc::clone(&kernel);
    let app_root = paths.root.clone();
    // Boot-time fail-fast: a failure to start the Tauri event loop is unrecoverable.
    #[allow(clippy::expect_used)]
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // T008/FR-014: a window opened while the mode is already on must be invisible too. Tauri 2
        // exposes no window-created event, but a new window always loads a page, and re-applying
        // protection is idempotent.
        .on_page_load(|webview, _| {
            let handle = webview.app_handle();
            let hidden = terminal_ai_persistence::dao::SettingsDao(
                &handle.state::<state::AppState>().database,
            )
            .get("invisible_mode")
            .ok()
            .flatten()
            .and_then(|value| serde_json::from_value::<bool>(value).ok())
            .unwrap_or(false);
            if hidden {
                if let Some(window) = handle.get_webview_window(webview.label()) {
                    let _ = invisible_mode::protect_window(&window, true);
                }
            }
        })
        .manage(state::AppState::new(
            database,
            paths.cache.clone(),
            paths.skills.clone(),
            paths.memory.clone(),
            exit_tx,
            kernel_for_state,
            app_root,
        ))
        .setup(move |app| {
            let handle = app.handle().clone();
            // One supervisor, one health loop. Started detached so nothing on the boot path waits
            // for the kernel — the UI opens with the kernel still starting, by design.
            {
                let kernel = std::sync::Arc::clone(&kernel_for_setup);
                let kernel_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if kernel_auto_start {
                        kernel
                            .handle(terminal_ai_memory_kernel::supervisor::Event::Start)
                            .await;
                    }
                    let _ = kernel_handle.emit("memory-kernel-status", kernel.status());
                    let mut previous = kernel.status().state;
                    let health = std::sync::Arc::clone(&kernel);
                    tauri::async_runtime::spawn(health.run_health_loop());
                    // Emit only on change: a status event every 15s would be noise the UI has to
                    // filter, and the snapshot is already readable on demand.
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        let status = kernel.status();
                        if status.state != previous {
                            previous = status.state;
                            let _ = kernel_handle.emit("memory-kernel-status", status);
                        }
                    }
                });
            }
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
            // T018/FR-008: the window is created hidden (tauri.conf.json), so the persisted mode is
            // in force before anything is on screen — there is no frame to capture. The `show()`
            // below is deliberately unconditional: a failure to apply must never leave the app
            // running with no window and no Dock icon, which is the one unrecoverable state.
            let boot_handle = app.handle().clone();
            let hidden = terminal_ai_persistence::dao::SettingsDao(
                &app.state::<state::AppState>().database,
            )
            .get("invisible_mode")
            .ok()
            .flatten()
            .and_then(|value| serde_json::from_value::<bool>(value).ok())
            .unwrap_or(false);
            if hidden {
                if let Err(error) =
                    tauri::async_runtime::block_on(invisible_mode::apply(&boot_handle, true))
                {
                    tracing::warn!(message = %error.message, "invisible mode: not restored at launch");
                }
            }
            for window in app.webview_windows().values() {
                let _ = window.show();
            }
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
            commands::update_memory,
            commands::delete_memory,
            commands::read_memory_page,
            commands::get_memory_kernel_status,
            commands::start_memory_kernel,
            commands::stop_memory_kernel,
            commands::restart_memory_kernel,
            commands::set_memory_kernel_settings,
            commands::set_memory_kernel_token,
            commands::run_memory_migration,
            commands::undo_memory_migration,
            commands::preview_memory_wiring,
            commands::apply_memory_wiring,
            commands::remove_memory_wiring,
            commands::list_memory_wiring,
            commands::check_memory_project_identity,
            commands::list_memory_handoffs,
            commands::expire_memory_handoffs,
            commands::get_memory_briefing,
            commands::get_settings,
            commands::set_settings,
            commands::notify
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Terminal AI")
        .run(move |_app, event| {
            // Stop the kernel on the way out — but only if we started it. A server the user was
            // already running keeps running (Constitution VII, SC-019).
            if matches!(event, tauri::RunEvent::Exit) {
                let kernel = std::sync::Arc::clone(&kernel_for_exit);
                tauri::async_runtime::block_on(async move {
                    kernel
                        .handle(terminal_ai_memory_kernel::supervisor::Event::AppExiting)
                        .await;
                });
            }
        });
}
