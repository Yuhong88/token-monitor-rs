#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod guardian;
mod models;
mod tray;
mod ui;

use guardian::{ensure_embedded_bridge, load_config, resolve_project_root, WorkerEvent};
use muda::MenuEvent;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
use tray::{TrayCommand, TrayController};
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
use ui::{apply_theme, setup_cjk_fonts, show_win32_window, SharedFlags, TokenMonitorApp};

const WINDOW_TITLE: &str = "Token Monitor RS";

fn main() -> eframe::Result<()> {
    let root = resolve_project_root();
    let _ = ensure_embedded_bridge(&root);
    let config = load_config(&root);

    let (tray_cmd_tx, tray_cmd_rx) = mpsc::channel::<TrayCommand>();
    let (worker_tx, worker_rx) = mpsc::channel::<WorkerEvent>();

    let tray = match TrayController::build() {
        Ok(t) => Some(t),
        Err(err) => {
            eprintln!("Failed to initialize system tray: {err}");
            None
        }
    };

    let shared = Arc::new(SharedFlags {
        hwnd: AtomicIsize::new(0),
        window_hidden: AtomicBool::new(false),
        allow_quit: AtomicBool::new(false),
    });

    let show_id = tray.as_ref().map(|t| t.show_id.clone());
    let refresh_id = tray.as_ref().map(|t| t.refresh_id.clone());
    let toggle_game_id = tray.as_ref().map(|t| t.toggle_game_id.clone());
    let sync_id = tray.as_ref().map(|t| t.sync_id.clone());
    let quit_id = tray.as_ref().map(|t| t.quit_id.clone());

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(WINDOW_TITLE)
            .with_inner_size([920.0, 680.0])
            .with_min_inner_size([700.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        WINDOW_TITLE,
        native_options,
        Box::new(move |cc| {
            setup_cjk_fonts(&cc.egui_ctx);
            apply_theme(&cc.egui_ctx);

            // Spawn lightweight listener thread to pump tray clicks & periodic wakeups even when hidden
            let egui_ctx = cc.egui_ctx.clone();
            let shared_bg = Arc::clone(&shared);
            let cmd_tx_bg = tray_cmd_tx.clone();
            std::thread::spawn(move || {
                let menu_rx = MenuEvent::receiver();
                let tray_rx = TrayIconEvent::receiver();
                let mut tick_counter: u32 = 0;

                loop {
                    let mut should_wake_ui = false;

                    while let Ok(ev) = menu_rx.try_recv() {
                        let cmd = if show_id.as_ref() == Some(&ev.id) {
                            Some(TrayCommand::ShowWindow)
                        } else if refresh_id.as_ref() == Some(&ev.id) {
                            Some(TrayCommand::RefreshNow)
                        } else if toggle_game_id.as_ref() == Some(&ev.id) {
                            Some(TrayCommand::ToggleGameMode)
                        } else if sync_id.as_ref() == Some(&ev.id) {
                            Some(TrayCommand::SyncUpstream)
                        } else if quit_id.as_ref() == Some(&ev.id) {
                            Some(TrayCommand::Quit)
                        } else {
                            None
                        };

                        if let Some(c) = cmd {
                            if c == TrayCommand::ShowWindow {
                                shared_bg.window_hidden.store(false, Ordering::Relaxed);
                                show_win32_window(shared_bg.hwnd.load(Ordering::Relaxed));
                            } else if c == TrayCommand::Quit {
                                shared_bg.allow_quit.store(true, Ordering::Relaxed);
                                shared_bg.window_hidden.store(false, Ordering::Relaxed);
                                show_win32_window(shared_bg.hwnd.load(Ordering::Relaxed));
                            }
                            let _ = cmd_tx_bg.send(c);
                            should_wake_ui = true;
                        }
                    }

                    while let Ok(ev) = tray_rx.try_recv() {
                        let open_requested = match ev {
                            TrayIconEvent::DoubleClick {
                                button: MouseButton::Left,
                                ..
                            } => true,
                            TrayIconEvent::Click {
                                button: MouseButton::Left,
                                button_state: MouseButtonState::Up,
                                ..
                            } => true,
                            _ => false,
                        };
                        if open_requested {
                            shared_bg.window_hidden.store(false, Ordering::Relaxed);
                            show_win32_window(shared_bg.hwnd.load(Ordering::Relaxed));
                            let _ = cmd_tx_bg.send(TrayCommand::ShowWindow);
                            should_wake_ui = true;
                        }
                    }

                    // Every ~10s, wake update loop lightly so background game-mode & refresh timers tick even when hidden
                    tick_counter = tick_counter.wrapping_add(1);
                    if should_wake_ui || tick_counter % 66 == 0 {
                        egui_ctx.request_repaint();
                    }

                    std::thread::sleep(Duration::from_millis(150));
                }
            });

            Ok(Box::new(TokenMonitorApp::new(
                root,
                config,
                tray,
                tray_cmd_rx,
                worker_tx,
                worker_rx,
                shared,
                WINDOW_TITLE.to_string(),
            )))
        }),
    )
}
