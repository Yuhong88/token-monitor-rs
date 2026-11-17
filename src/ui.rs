use crate::guardian::{
    active_game_reason, compute_watched_fingerprint, load_snapshot, save_config,
    spawn_collector_async, spawn_git_pull_async, LogFingerprint, WorkerEvent,
};
use crate::models::{
    AppConfig, GameModePolicy, LimitProvider, PeriodKind, Snapshot, ToolUsage,
};
use crate::tray::{TrayCommand, TrayController};
use chrono::{DateTime, Local, Utc};
use eframe::egui;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(windows)]
pub fn show_win32_window(hwnd_val: isize) {
    if hwnd_val == 0 {
        return;
    }
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
    };
    unsafe {
        let hwnd = hwnd_val as windows_sys::Win32::Foundation::HWND;
        ShowWindow(hwnd, SW_RESTORE);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
    }
}

#[cfg(windows)]
pub fn hide_win32_window(hwnd_val: isize) {
    if hwnd_val == 0 {
        return;
    }
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    unsafe {
        let hwnd = hwnd_val as windows_sys::Win32::Foundation::HWND;
        ShowWindow(hwnd, SW_HIDE);
    }
}

#[cfg(windows)]
pub fn find_app_hwnd(title: &str) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;
    let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let hwnd = unsafe { FindWindowW(std::ptr::null(), wide.as_ptr()) };
    hwnd as isize
}

#[cfg(not(windows))]
pub fn show_win32_window(_hwnd_val: isize) {}
#[cfg(not(windows))]
pub fn hide_win32_window(_hwnd_val: isize) {}
#[cfg(not(windows))]
pub fn find_app_hwnd(_title: &str) -> isize {
    0
}

pub fn setup_cjk_fonts(ctx: &egui::Context) {
    let candidates = [
        r"C:\Windows\Fonts\msjh.ttc",
        r"C:\Windows\Fonts\msjhbd.ttc",
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\mingliu.ttc",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "cjk_ui".to_owned(),
                egui::FontData::from_owned(bytes).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "cjk_ui".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("cjk_ui".to_owned());
            ctx.set_fonts(fonts);
            return;
        }
    }
}

pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = egui::Color32::from_rgb(11, 15, 23);
    visuals.window_fill = egui::Color32::from_rgb(15, 21, 32);
    visuals.faint_bg_color = egui::Color32::from_rgb(19, 26, 38);
    visuals.extreme_bg_color = egui::Color32::from_rgb(8, 11, 18);
    visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(19, 26, 38);
    visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(34, 46, 66));
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(26, 36, 52);
    visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(44, 58, 82));
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(36, 50, 72);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(46, 64, 92);
    visuals.selection.bg_fill = egui::Color32::from_rgb(29, 78, 216);
    visuals.window_rounding = egui::Rounding::same(10.0);
    visuals.widgets.noninteractive.rounding = egui::Rounding::same(8.0);
    visuals.widgets.inactive.rounding = egui::Rounding::same(6.0);
    visuals.widgets.hovered.rounding = egui::Rounding::same(6.0);
    visuals.widgets.active.rounding = egui::Rounding::same(6.0);
    ctx.set_visuals(visuals);
}

pub struct SharedFlags {
    pub hwnd: AtomicIsize,
    pub window_hidden: AtomicBool,
    pub allow_quit: AtomicBool,
}

pub struct TokenMonitorApp {
    pub root: PathBuf,
    pub config: AppConfig,
    pub snapshot: Option<Snapshot>,
    pub tray: Option<TrayController>,
    pub tray_cmd_rx: Receiver<TrayCommand>,
    pub worker_tx: Sender<WorkerEvent>,
    pub worker_rx: Receiver<WorkerEvent>,
    pub shared: Arc<SharedFlags>,

    pub is_collecting: bool,
    pub is_syncing: bool,
    pub game_pause_reason: Option<String>,
    pub last_fingerprint: LogFingerprint,
    pub last_collect_attempt: Instant,
    pub last_game_check: Instant,
    pub startup_time: Instant,
    pub status_banner: String,
    pub expanded_tools: HashSet<String>,
    pub new_game_exe_input: String,
    pub window_title: String,
}

impl TokenMonitorApp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        root: PathBuf,
        config: AppConfig,
        tray: Option<TrayController>,
        tray_cmd_rx: Receiver<TrayCommand>,
        worker_tx: Sender<WorkerEvent>,
        worker_rx: Receiver<WorkerEvent>,
        shared: Arc<SharedFlags>,
        window_title: String,
    ) -> Self {
        let snapshot = load_snapshot(&root);
        let fp = compute_watched_fingerprint();
        let game_reason = active_game_reason(&config);

        Self {
            root,
            config,
            snapshot,
            tray,
            tray_cmd_rx,
            worker_tx,
            worker_rx,
            shared,
            is_collecting: false,
            is_syncing: false,
            game_pause_reason: game_reason,
            last_fingerprint: fp,
            last_collect_attempt: Instant::now(),
            last_game_check: Instant::now(),
            startup_time: Instant::now(),
            status_banner: "就緒 (輕量常駐模式)".to_string(),
            expanded_tools: HashSet::new(),
            new_game_exe_input: String::new(),
            window_title,
        }
    }

    pub fn trigger_collect(&mut self, ctx: &egui::Context, force: bool) {
        if self.is_collecting {
            return;
        }
        if !force {
            if let Some(reason) = &self.game_pause_reason {
                self.status_banner = format!("休眠中 ({reason}) — 已跳過掃描");
                return;
            }
            let current_fp = compute_watched_fingerprint();
            if self.snapshot.is_some() && current_fp == self.last_fingerprint {
                self.last_collect_attempt = Instant::now();
                self.status_banner = "日誌無變動，已跳過硬碟掃描 (0 I/O)".to_string();
                return;
            }
            self.last_fingerprint = current_fp;
        } else {
            self.last_fingerprint = compute_watched_fingerprint();
        }

        self.is_collecting = true;
        self.last_collect_attempt = Instant::now();
        self.status_banner = "正在透過原作者橋接器採集最新數據...".to_string();
        spawn_collector_async(self.root.clone(), self.worker_tx.clone(), ctx.clone());
    }

    pub fn trigger_sync(&mut self, ctx: &egui::Context) {
        if self.is_syncing {
            return;
        }
        self.is_syncing = true;
        self.config.last_upstream_check_epoch = Utc::now().timestamp().max(0) as u64;
        save_config(&self.root, &self.config);
        self.status_banner = "正在檢查並同步原作者最新監測庫...".to_string();
        spawn_git_pull_async(self.root.clone(), self.worker_tx.clone(), ctx.clone());
    }

    fn refresh_tray_tooltip(&self) {
        let Some(tray) = &self.tray else {
            return;
        };
        if let Some(reason) = &self.game_pause_reason {
            tray.update_tooltip(&format!("Token Monitor RS — 遊戲休眠中 ({reason})"));
            return;
        }
        if let Some(snap) = &self.snapshot {
            let today = &snap.periods.today;
            tray.update_tooltip(&format!(
                "Token Monitor RS | 今日: {} tok (${:.4})",
                format_compact_tokens(today.total_tokens),
                today.cost_usd
            ));
        } else {
            tray.update_tooltip("Token Monitor RS — 輕量監控中");
        }
    }

    fn hide_to_tray(&self) {
        self.shared.window_hidden.store(true, Ordering::Relaxed);
        let hwnd = self.shared.hwnd.load(Ordering::Relaxed);
        if hwnd != 0 {
            hide_win32_window(hwnd);
        }
    }

    fn handle_background_events(&mut self, ctx: &egui::Context) {
        // 1. Drain worker events
        while let Ok(ev) = self.worker_rx.try_recv() {
            match ev {
                WorkerEvent::CollectFinished {
                    ok,
                    message,
                    snapshot,
                } => {
                    self.is_collecting = false;
                    self.status_banner = message;
                    if ok {
                        if let Some(snap) = snapshot {
                            self.snapshot = Some(snap);
                        }
                    }
                    self.refresh_tray_tooltip();
                }
                WorkerEvent::SyncFinished {
                    ok,
                    updated,
                    message,
                } => {
                    self.is_syncing = false;
                    self.status_banner = message;
                    if ok && (updated || self.snapshot.is_none()) {
                        self.trigger_collect(ctx, true);
                    }
                }
            }
        }

        // 2. Drain tray menu/click commands
        while let Ok(cmd) = self.tray_cmd_rx.try_recv() {
            match cmd {
                TrayCommand::ShowWindow => {
                    self.shared.window_hidden.store(false, Ordering::Relaxed);
                    let hwnd = self.shared.hwnd.load(Ordering::Relaxed);
                    show_win32_window(hwnd);
                }
                TrayCommand::RefreshNow => {
                    self.trigger_collect(ctx, true);
                }
                TrayCommand::ToggleGameMode => {
                    self.config.game_mode = match self.config.game_mode {
                        GameModePolicy::Auto => GameModePolicy::ForceSleep,
                        GameModePolicy::ForceSleep => GameModePolicy::Off,
                        GameModePolicy::Off => GameModePolicy::Auto,
                    };
                    save_config(&self.root, &self.config);
                    self.game_pause_reason = active_game_reason(&self.config);
                    self.status_banner = format!("模式切換為: {}", self.config.game_mode.label());
                    self.refresh_tray_tooltip();
                }
                TrayCommand::SyncUpstream => {
                    self.trigger_sync(ctx);
                }
                TrayCommand::Quit => {
                    self.shared.allow_quit.store(true, Ordering::Relaxed);
                    self.shared.window_hidden.store(false, Ordering::Relaxed);
                    let hwnd = self.shared.hwnd.load(Ordering::Relaxed);
                    show_win32_window(hwnd);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        // 3. Periodic Game-Mode Check (every 5 seconds)
        if self.last_game_check.elapsed() >= Duration::from_secs(5) {
            self.last_game_check = Instant::now();
            let next_reason = active_game_reason(&self.config);
            if next_reason != self.game_pause_reason {
                self.game_pause_reason = next_reason;
                self.refresh_tray_tooltip();
            }
        }

        // 4. Periodic Auto Refresh (only when interval > 0 and NOT gaming)
        if self.config.refresh_interval_mins > 0 && self.game_pause_reason.is_none() {
            let interval = Duration::from_secs(self.config.refresh_interval_mins * 60);
            if self.last_collect_attempt.elapsed() >= interval {
                self.trigger_collect(ctx, false);
            }
        }

        // 5. Built-in Zero-AI Upstream Auto-Updater (after 8s startup grace, paused while gaming)
        if self.config.auto_update_upstream
            && self.config.auto_update_hours > 0
            && self.game_pause_reason.is_none()
            && !self.is_syncing
            && self.startup_time.elapsed() >= Duration::from_secs(8)
        {
            let now_epoch = Utc::now().timestamp().max(0) as u64;
            let interval_secs = self.config.auto_update_hours * 3600;
            if now_epoch.saturating_sub(self.config.last_upstream_check_epoch) >= interval_secs {
                self.trigger_sync(ctx);
            }
        }
    }
}

impl eframe::App for TokenMonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Lazily capture Win32 HWND once window exists
        if self.shared.hwnd.load(Ordering::Relaxed) == 0 {
            let hwnd = find_app_hwnd(&self.window_title);
            if hwnd != 0 {
                self.shared.hwnd.store(hwnd, Ordering::Relaxed);
            }
        }

        // Intercept close button -> minimize to tray unless Quit was requested
        if ctx.input(|i| i.viewport().close_requested()) {
            if !self.shared.allow_quit.load(Ordering::Relaxed) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.hide_to_tray();
                return;
            }
        }

        self.handle_background_events(ctx);

        // If hidden to tray, don't burn GPU painting widgets; only check gently if needed
        if self.shared.window_hidden.load(Ordering::Relaxed) {
            return;
        }

        // Keep UI responsive while collecting or displaying live countdowns
        if self.is_collecting || self.is_syncing {
            ctx.request_repaint_after(Duration::from_millis(250));
        } else {
            ctx.request_repaint_after(Duration::from_secs(5));
        }

        // Top Header Bar
        egui::TopBottomPanel::top("header_bar")
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(15, 21, 32))
                    .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                    .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(30, 41, 59))),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("Token Monitor RS")
                            .size(20.0)
                            .strong()
                            .color(egui::Color32::from_rgb(241, 245, 249)),
                    );
                    pill_badge(
                        ui,
                        "極輕量常駐版",
                        egui::Color32::from_rgb(30, 58, 138),
                        egui::Color32::from_rgb(147, 197, 253),
                    );

                    if let Some(snap) = &self.snapshot {
                        if !snap.upstream_version.is_empty() {
                            pill_badge(
                                ui,
                                &format!(
                                    "原作者 core v{} ({})",
                                    snap.upstream_version, snap.upstream_commit
                                ),
                                egui::Color32::from_rgb(30, 41, 59),
                                egui::Color32::from_rgb(148, 163, 184),
                            );
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("縮到系統匣").clicked() {
                            self.hide_to_tray();
                        }

                        let refresh_btn = ui.add_enabled(
                            !self.is_collecting,
                            egui::Button::new(if self.is_collecting {
                                "採集中..."
                            } else {
                                "↻ 立即刷新"
                            }),
                        );
                        if refresh_btn.clicked() {
                            self.trigger_collect(ctx, true);
                        }

                        let sync_btn = ui.add_enabled(
                            !self.is_syncing,
                            egui::Button::new(if self.is_syncing {
                                "同步中..."
                            } else {
                                "↑ 檢查並同步作者更新"
                            }),
                        );
                        if sync_btn.clicked() {
                            self.trigger_sync(ctx);
                        }

                        if self.is_collecting || self.is_syncing {
                            pill_badge(
                                ui,
                                "● 採集更新中",
                                egui::Color32::from_rgb(23, 37, 84),
                                egui::Color32::from_rgb(96, 165, 250),
                            );
                        } else if self.game_pause_reason.is_some() {
                            pill_badge(
                                ui,
                                "● 遊戲休眠中 (0 I/O)",
                                egui::Color32::from_rgb(69, 26, 3),
                                egui::Color32::from_rgb(251, 191, 36),
                            );
                        } else {
                            pill_badge(
                                ui,
                                "● 輕量待機中",
                                egui::Color32::from_rgb(6, 78, 59),
                                egui::Color32::from_rgb(52, 211, 153),
                            );
                        }
                    });
                });
            });

        // Bottom Status Bar
        egui::TopBottomPanel::bottom("footer_bar")
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(15, 21, 32))
                    .inner_margin(egui::Margin::symmetric(16.0, 8.0))
                    .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(30, 41, 59))),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let updated = self
                        .snapshot
                        .as_ref()
                        .map(|s| format_local_time(&s.updated_at))
                        .unwrap_or_else(|| "尚未採集".to_string());
                    ui.label(
                        egui::RichText::new(format!("上次更新: {updated}   |   狀態: {}", self.status_banner))
                            .size(12.0)
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(
                                egui::RichText::new("完全退出")
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(248, 113, 113)),
                            )
                            .clicked()
                        {
                            self.shared.allow_quit.store(true, Ordering::Relaxed);
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
            });

        // Main Scrollable Content
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.add_space(6.0);

                    // Control Bar
                    self.render_control_bar(ui);
                    ui.add_space(12.0);

                    let Some(snap) = self.snapshot.clone() else {
                        egui::Frame::none()
                            .fill(egui::Color32::from_rgb(19, 26, 38))
                            .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(34, 46, 66)))
                            .rounding(8.0)
                            .inner_margin(24.0)
                            .show(ui, |ui| {
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        egui::RichText::new("尚未讀取到用量快照，請點擊右上角「↻ 立即刷新」")
                                            .size(15.0)
                                            .color(egui::Color32::from_rgb(203, 213, 225)),
                                    );
                                });
                            });
                        return;
                    };

                    let period = snap.periods.get(self.config.selected_period);

                    // KPI Cards (4 columns)
                    self.render_kpi_cards(ui, period);
                    ui.add_space(16.0);

                    // Section 1: Live Quotas & Rate Limits
                    self.render_limits_section(ui, &snap.limits);
                    ui.add_space(16.0);

                    // Section 2: 7 Target Tools & Models
                    self.render_tools_section(ui, &period.tools);
                    ui.add_space(16.0);

                    // Section 3: Recent Sessions (Collapsible to avoid clutter)
                    self.render_recent_sessions(ui, &snap);
                    ui.add_space(10.0);

                    // Section 4: Game Mode Custom Exe List (Collapsible)
                    self.render_game_settings(ui);
                    ui.add_space(12.0);
                });
        });
    }
}

impl TokenMonitorApp {
    fn render_control_bar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(19, 26, 38))
            .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(34, 46, 66)))
            .rounding(8.0)
            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for p in [
                        PeriodKind::Today,
                        PeriodKind::Week,
                        PeriodKind::Month,
                        PeriodKind::AllTime,
                    ] {
                        let selected = self.config.selected_period == p;
                        if ui.selectable_label(selected, p.label()).clicked() {
                            self.config.selected_period = p;
                            save_config(&self.root, &self.config);
                        }
                    }

                    ui.separator();

                    if ui
                        .checkbox(&mut self.config.hide_inactive_tools, "隱藏 0 用量工具")
                        .changed()
                    {
                        save_config(&self.root, &self.config);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Refresh interval selector
                        let interval_label = match self.config.refresh_interval_mins {
                            0 => "僅手動刷新".to_string(),
                            m => format!("每 {m} 分鐘檢查"),
                        };
                        egui::ComboBox::from_id_salt("refresh_interval_combo")
                            .selected_text(interval_label)
                            .width(120.0)
                            .show_ui(ui, |ui| {
                                for (mins, text) in [
                                    (5, "每 5 分鐘檢查"),
                                    (15, "每 15 分鐘檢查"),
                                    (30, "每 30 分鐘檢查"),
                                    (0, "僅手動刷新 (最省資源)"),
                                ] {
                                    if ui
                                        .selectable_value(
                                            &mut self.config.refresh_interval_mins,
                                            mins,
                                            text,
                                        )
                                        .clicked()
                                    {
                                        save_config(&self.root, &self.config);
                                    }
                                }
                            });

                        // Auto-updater schedule selector
                        let auto_update_label = if !self.config.auto_update_upstream || self.config.auto_update_hours == 0 {
                            "作者更新: 僅手動".to_string()
                        } else {
                            format!("自動更新: 每 {}h", self.config.auto_update_hours)
                        };
                        egui::ComboBox::from_id_salt("auto_update_combo")
                            .selected_text(auto_update_label)
                            .width(145.0)
                            .show_ui(ui, |ui| {
                                for (hours, text) in [
                                    (6_u64, "自動抓取作者更新: 每 6 小時"),
                                    (12_u64, "自動抓取作者更新: 每 12 小時"),
                                    (24_u64, "自動抓取作者更新: 每 24 小時"),
                                    (0_u64, "僅手動點擊更新"),
                                ] {
                                    let selected = if hours == 0 {
                                        !self.config.auto_update_upstream || self.config.auto_update_hours == 0
                                    } else {
                                        self.config.auto_update_upstream && self.config.auto_update_hours == hours
                                    };
                                    if ui.selectable_label(selected, text).clicked() {
                                        self.config.auto_update_hours = hours;
                                        self.config.auto_update_upstream = hours > 0;
                                        save_config(&self.root, &self.config);
                                    }
                                }
                            });

                        // Game mode policy selector
                        egui::ComboBox::from_id_salt("game_mode_combo")
                            .selected_text(self.config.game_mode.label())
                            .width(145.0)
                            .show_ui(ui, |ui| {
                                for policy in [
                                    GameModePolicy::Auto,
                                    GameModePolicy::ForceSleep,
                                    GameModePolicy::Off,
                                ] {
                                    if ui
                                        .selectable_value(
                                            &mut self.config.game_mode,
                                            policy,
                                            policy.label(),
                                        )
                                        .clicked()
                                    {
                                        save_config(&self.root, &self.config);
                                        self.game_pause_reason = active_game_reason(&self.config);
                                        self.refresh_tray_tooltip();
                                    }
                                }
                            });
                    });
                });
            });
    }

    fn render_kpi_cards(&self, ui: &mut egui::Ui, period: &crate::models::PeriodData) {
        let active_count = period.tools.iter().filter(|t| t.total_tokens > 0).count();
        let cache_hit_denom = period.input_tokens + period.cache_read_tokens;
        let cache_hit_pct = if cache_hit_denom > 0 {
            (period.cache_read_tokens as f64 / cache_hit_denom as f64) * 100.0
        } else {
            0.0
        };
        let tok_per_sec = if period.timed_duration_ms > 0 && period.timed_output_tokens > 0 {
            format!(
                "{:.1} tok/s 平均生成",
                period.timed_output_tokens as f64 / (period.timed_duration_ms as f64 / 1000.0)
            )
        } else {
            format!("含推理 {}", format_compact_tokens(period.reasoning_tokens))
        };

        ui.columns(4, |cols| {
            kpi_card(
                &mut cols[0],
                "預估花費 (USD)",
                &format!("${:.4}", period.cost_usd),
                &format!("{} 個活耀工具", active_count),
                egui::Color32::from_rgb(52, 211, 153),
            );
            kpi_card(
                &mut cols[1],
                "總 Token 用量",
                &format_compact_tokens(period.total_tokens),
                &format!("精確: {}", format_grouped(period.total_tokens)),
                egui::Color32::from_rgb(56, 189, 248),
            );
            kpi_card(
                &mut cols[2],
                "輸入 & 快取讀取",
                &format!(
                    "{} / {}",
                    format_compact_tokens(period.input_tokens),
                    format_compact_tokens(period.cache_read_tokens)
                ),
                &format!("快取命中率 {:.1}%", cache_hit_pct),
                egui::Color32::from_rgb(167, 139, 250),
            );
            kpi_card(
                &mut cols[3],
                "輸出 Tokens",
                &format_compact_tokens(period.output_tokens),
                &tok_per_sec,
                egui::Color32::from_rgb(251, 191, 36),
            );
        });
    }

    fn render_limits_section(&self, ui: &mut egui::Ui, limits: &[LimitProvider]) {
        ui.label(
            egui::RichText::new("即時額度與速率限制 (Quotas & Rate Limits)")
                .size(15.0)
                .strong()
                .color(egui::Color32::from_rgb(226, 232, 240)),
        );
        ui.add_space(6.0);

        if limits.is_empty() {
            ui.label(
                egui::RichText::new("目前沒有額度追蹤資料（僅針對 OpenCode / Claude / Antigravity / Codex 探測）")
                    .size(12.5)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            return;
        }

        // 2-column grid for provider quota cards
        for chunk in limits.chunks(2) {
            ui.columns(2, |cols| {
                for (idx, provider) in chunk.iter().enumerate() {
                    render_limit_provider_card(&mut cols[idx], provider);
                }
            });
            ui.add_space(8.0);
        }
    }

    fn render_tools_section(&mut self, ui: &mut egui::Ui, tools: &[ToolUsage]) {
        let visible_tools: Vec<&ToolUsage> = tools
            .iter()
            .filter(|t| !self.config.hide_inactive_tools || t.total_tokens > 0 || t.cost_usd > 0.0)
            .collect();

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("7 大主力工具與模型明細")
                    .size(15.0)
                    .strong()
                    .color(egui::Color32::from_rgb(226, 232, 240)),
            );
            ui.label(
                egui::RichText::new(format!("(顯示 {} / {} 個工具)", visible_tools.len(), tools.len()))
                    .size(12.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            );
        });
        ui.add_space(6.0);

        if visible_tools.is_empty() {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(19, 26, 38))
                .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(34, 46, 66)))
                .rounding(8.0)
                .inner_margin(14.0)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("此區間沒有用量紀錄（可切換上方「總計 All-Time」或取消勾選「隱藏 0 用量工具」）")
                            .size(13.0)
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                });
            return;
        }

        for tool in visible_tools {
            let expanded = self.expanded_tools.contains(&tool.id);
            let has_models = !tool.models.is_empty();

            egui::Frame::none()
                .fill(egui::Color32::from_rgb(19, 26, 38))
                .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(34, 46, 66)))
                .rounding(8.0)
                .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (dot_color, status_text) = tool_status_badge(&tool.status, tool.total_tokens);
                        ui.label(egui::RichText::new("●").size(12.0).color(dot_color));
                        ui.label(
                            egui::RichText::new(&tool.label)
                                .size(14.5)
                                .strong()
                                .color(egui::Color32::from_rgb(241, 245, 249)),
                        );
                        pill_badge(
                            ui,
                            status_text,
                            egui::Color32::from_rgb(26, 36, 52),
                            dot_color,
                        );

                        if tool.session_count > 0 {
                            ui.label(
                                egui::RichText::new(format!("{} 次對話", tool.session_count))
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                        }
                        if !tool.last_used_at.is_empty() {
                            ui.label(
                                egui::RichText::new(format!("最後活躍: {}", format_local_time(&tool.last_used_at)))
                                    .size(11.5)
                                    .color(egui::Color32::from_rgb(100, 116, 139)),
                            );
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if has_models {
                                let btn_text = if expanded { "▾ 收合模型" } else { "▸ 模型明細" };
                                if ui.small_button(btn_text).clicked() {
                                    if expanded {
                                        self.expanded_tools.remove(&tool.id);
                                    } else {
                                        self.expanded_tools.insert(tool.id.clone());
                                    }
                                }
                            }
                            ui.label(
                                egui::RichText::new(format!("${:.4}", tool.cost_usd))
                                    .size(14.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(52, 211, 153)),
                            );
                            ui.add_space(10.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} tokens",
                                    format_compact_tokens(tool.total_tokens)
                                ))
                                .size(14.0)
                                .strong()
                                .color(egui::Color32::from_rgb(226, 232, 240)),
                            );
                        });
                    });

                    if tool.total_tokens > 0 {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "輸入: {}   ·   快取讀: {}   ·   快取寫: {}   ·   輸出: {}   ·   推理: {}",
                                    format_compact_tokens(tool.input_tokens),
                                    format_compact_tokens(tool.cache_read_tokens),
                                    format_compact_tokens(tool.cache_write_tokens),
                                    format_compact_tokens(tool.output_tokens),
                                    format_compact_tokens(tool.reasoning_tokens),
                                ))
                                .size(12.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                            );
                        });
                    }

                    if expanded && has_models {
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);
                        egui::Grid::new(format!("models_grid_{}", tool.id))
                            .num_columns(4)
                            .spacing([18.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("模型").size(12.0).color(egui::Color32::from_rgb(148, 163, 184)));
                                ui.label(egui::RichText::new("Token").size(12.0).color(egui::Color32::from_rgb(148, 163, 184)));
                                ui.label(egui::RichText::new("占比").size(12.0).color(egui::Color32::from_rgb(148, 163, 184)));
                                ui.label(egui::RichText::new("花費").size(12.0).color(egui::Color32::from_rgb(148, 163, 184)));
                                ui.end_row();

                                for m in &tool.models {
                                    let share = if tool.total_tokens > 0 {
                                        (m.tokens as f64 / tool.total_tokens as f64) * 100.0
                                    } else {
                                        0.0
                                    };
                                    ui.label(
                                        egui::RichText::new(&m.name)
                                            .size(12.5)
                                            .monospace()
                                            .color(egui::Color32::from_rgb(226, 232, 240)),
                                    );
                                    ui.label(
                                        egui::RichText::new(format_compact_tokens(m.tokens))
                                            .size(12.5)
                                            .color(egui::Color32::from_rgb(203, 213, 225)),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!("{share:.1}%"))
                                            .size(12.5)
                                            .color(egui::Color32::from_rgb(148, 163, 184)),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!("${:.4}", m.cost_usd))
                                            .size(12.5)
                                            .color(egui::Color32::from_rgb(52, 211, 153)),
                                    );
                                    ui.end_row();
                                }
                            });
                    }
                });
            ui.add_space(6.0);
        }
    }

    fn render_recent_sessions(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("近期活躍對話 ({})", snap.recent_sessions.len()))
                .size(14.0)
                .strong()
                .color(egui::Color32::from_rgb(226, 232, 240)),
        )
        .default_open(false)
        .show(ui, |ui| {
            if snap.recent_sessions.is_empty() {
                ui.label(
                    egui::RichText::new("暫無近期對話紀錄")
                        .size(12.5)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
                return;
            }
            egui::Grid::new("recent_sessions_grid")
                .num_columns(5)
                .spacing([14.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    for h in ["工具", "標題 / 專案", "模型", "Token / 花費", "最後使用"] {
                        ui.label(
                            egui::RichText::new(h)
                                .size(12.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                    }
                    ui.end_row();

                    for s in &snap.recent_sessions {
                        ui.label(
                            egui::RichText::new(&s.client)
                                .size(12.5)
                                .strong()
                                .color(egui::Color32::from_rgb(56, 189, 248)),
                        );
                        let title_short: String = s.title.chars().take(34).collect();
                        ui.label(
                            egui::RichText::new(title_short)
                                .size(12.5)
                                .color(egui::Color32::from_rgb(226, 232, 240)),
                        );
                        ui.label(
                            egui::RichText::new(&s.latest_model)
                                .size(12.0)
                                .monospace()
                                .color(egui::Color32::from_rgb(203, 213, 225)),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · ${:.4}",
                                format_compact_tokens(s.total_tokens),
                                s.cost_usd
                            ))
                            .size(12.5)
                            .color(egui::Color32::from_rgb(52, 211, 153)),
                        );
                        ui.label(
                            egui::RichText::new(format_local_time(&s.last_used_at))
                                .size(12.0)
                                .color(egui::Color32::from_rgb(148, 163, 184)),
                        );
                        ui.end_row();
                    }
                });
        });
    }

    fn render_game_settings(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(
            egui::RichText::new("遊戲模式偵測設定 (偵測全螢幕或自訂遊戲 .exe 自動 0 I/O 休眠)")
                .size(13.5)
                .color(egui::Color32::from_rgb(203, 213, 225)),
        )
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("除 Windows 自動全螢幕 (Direct3D) 偵測外，若以下清單的遊戲進程正在執行，也會自動進入休眠：")
                    .size(12.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_game_exe_input)
                        .hint_text("輸入遊戲執行檔名，例如: GenshinImpact.exe")
                        .desired_width(280.0),
                );
                if ui.button("新增遊戲").clicked() {
                    let trimmed = self.new_game_exe_input.trim().to_string();
                    if !trimmed.is_empty()
                        && !self
                            .config
                            .custom_game_processes
                            .iter()
                            .any(|x| x.eq_ignore_ascii_case(&trimmed))
                    {
                        self.config.custom_game_processes.push(trimmed);
                        self.new_game_exe_input.clear();
                        save_config(&self.root, &self.config);
                        self.game_pause_reason = active_game_reason(&self.config);
                    }
                }
            });
            ui.add_space(6.0);
            let mut remove_idx = None;
            ui.horizontal_wrapped(|ui| {
                for (i, exe) in self.config.custom_game_processes.iter().enumerate() {
                    if ui.small_button(format!("{exe}  ×")).clicked() {
                        remove_idx = Some(i);
                    }
                }
            });
            if let Some(i) = remove_idx {
                self.config.custom_game_processes.remove(i);
                save_config(&self.root, &self.config);
                self.game_pause_reason = active_game_reason(&self.config);
            }
        });
    }
}

fn kpi_card(
    ui: &mut egui::Ui,
    title: &str,
    value: &str,
    sub: &str,
    accent: egui::Color32,
) {
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(19, 26, 38))
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(34, 46, 66)))
        .rounding(8.0)
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                egui::RichText::new(title)
                    .size(12.0)
                    .color(egui::Color32::from_rgb(148, 163, 184)),
            );
            ui.add_space(4.0);
            ui.label(egui::RichText::new(value).size(22.0).strong().color(accent));
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(sub)
                    .size(11.5)
                    .color(egui::Color32::from_rgb(100, 116, 139)),
            );
        });
}

fn render_limit_provider_card(ui: &mut egui::Ui, provider: &LimitProvider) {
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(19, 26, 38))
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(34, 46, 66)))
        .rounding(8.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&provider.label)
                        .size(14.0)
                        .strong()
                        .color(egui::Color32::from_rgb(241, 245, 249)),
                );
                if !provider.account_label.is_empty() {
                    pill_badge(
                        ui,
                        &provider.account_label,
                        egui::Color32::from_rgb(30, 41, 59),
                        egui::Color32::from_rgb(147, 197, 253),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (st_text, st_fg, st_bg) = limit_status_style(&provider.status);
                    pill_badge(ui, st_text, st_bg, st_fg);
                    if let Some(bal) = provider.balance_usd {
                        let cur = provider.balance_currency.as_deref().unwrap_or("USD");
                        ui.label(
                            egui::RichText::new(format!("餘額: {bal:.2} {cur}"))
                                .size(12.0)
                                .color(egui::Color32::from_rgb(52, 211, 153)),
                        );
                    }
                });
            });

            if !provider.account_email.is_empty() || !provider.account_name.is_empty() {
                let id_str = if !provider.account_email.is_empty() {
                    &provider.account_email
                } else {
                    &provider.account_name
                };
                ui.label(
                    egui::RichText::new(id_str)
                        .size(11.5)
                        .color(egui::Color32::from_rgb(100, 116, 139)),
                );
            }

            if provider.windows.is_empty() {
                ui.add_space(4.0);
                let hint = match provider.status.as_str() {
                    "notConfigured" => "尚未登入或無本地憑證",
                    "unavailable" => "目前無法連線探測（請確認對應工具已啟動）",
                    _ => "暫無額度視窗資料",
                };
                ui.label(
                    egui::RichText::new(hint)
                        .size(12.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                );
            } else {
                ui.add_space(6.0);
                for w in &provider.windows {
                    let win_title = if w.label.is_empty() {
                        match w.kind.as_str() {
                            "session" => "5 小時視窗".to_string(),
                            "daily" => "每日額度".to_string(),
                            "weekly" => "每週額度".to_string(),
                            "billing" => "計費週期".to_string(),
                            other => other.to_string(),
                        }
                    } else {
                        w.label.clone()
                    };

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(win_title)
                                .size(12.5)
                                .color(egui::Color32::from_rgb(203, 213, 225)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(resets) = &w.resets_at {
                                ui.label(
                                    egui::RichText::new(format_countdown(resets))
                                        .size(11.5)
                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                            } else if !w.reset_description.is_empty() {
                                ui.label(
                                    egui::RichText::new(&w.reset_description)
                                        .size(11.5)
                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                );
                            }
                            if let Some(used_pct) = w.used_percent {
                                let rem_pct = w.remaining_percent.unwrap_or((100.0 - used_pct).max(0.0));
                                ui.label(
                                    egui::RichText::new(format!("已用 {used_pct:.0}% · 剩 {rem_pct:.0}%"))
                                        .size(12.0)
                                        .strong()
                                        .color(progress_color(used_pct)),
                                );
                            } else if w.metric.as_deref() == Some("credits") {
                                if let Some(rem) = w.remaining {
                                    let cur = w.currency.as_deref().unwrap_or("USD");
                                    ui.label(
                                        egui::RichText::new(format!("剩餘 {rem:.2} {cur}"))
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(52, 211, 153)),
                                    );
                                }
                            }
                        });
                    });

                    if w.show_meter {
                        if let Some(used_pct) = w.used_percent {
                            let frac = (used_pct / 100.0).clamp(0.0, 1.0) as f32;
                            let bar = egui::ProgressBar::new(frac)
                                .desired_height(6.0)
                                .fill(progress_color(used_pct));
                            ui.add(bar);
                        }
                    }
                    ui.add_space(4.0);
                }
            }
        });
}

fn pill_badge(ui: &mut egui::Ui, text: &str, bg: egui::Color32, fg: egui::Color32) {
    egui::Frame::none()
        .fill(bg)
        .rounding(6.0)
        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(11.5).color(fg));
        });
}

fn progress_color(used_pct: f64) -> egui::Color32 {
    if used_pct >= 85.0 {
        egui::Color32::from_rgb(248, 113, 113)
    } else if used_pct >= 60.0 {
        egui::Color32::from_rgb(251, 191, 36)
    } else {
        egui::Color32::from_rgb(52, 211, 153)
    }
}

fn limit_status_style(status: &str) -> (&'static str, egui::Color32, egui::Color32) {
    match status {
        "ok" => (
            "正常",
            egui::Color32::from_rgb(52, 211, 153),
            egui::Color32::from_rgb(6, 78, 59),
        ),
        "rateLimited" | "sourceRateLimited" => (
            "受限",
            egui::Color32::from_rgb(248, 113, 113),
            egui::Color32::from_rgb(69, 10, 10),
        ),
        "unauthorized" => (
            "需重新登入",
            egui::Color32::from_rgb(251, 191, 36),
            egui::Color32::from_rgb(69, 26, 3),
        ),
        "notConfigured" => (
            "未設定",
            egui::Color32::from_rgb(148, 163, 184),
            egui::Color32::from_rgb(30, 41, 59),
        ),
        _ => (
            "等待連線",
            egui::Color32::from_rgb(251, 191, 36),
            egui::Color32::from_rgb(69, 26, 3),
        ),
    }
}

fn tool_status_badge(status: &str, tokens: u64) -> (egui::Color32, &'static str) {
    if tokens > 0 || status == "active" {
        (egui::Color32::from_rgb(52, 211, 153), "已偵測")
    } else if status == "waiting" {
        (egui::Color32::from_rgb(251, 191, 36), "待命中")
    } else if status == "standby" {
        (egui::Color32::from_rgb(148, 163, 184), "待命槽")
    } else {
        (egui::Color32::from_rgb(100, 116, 139), "無資料")
    }
}

pub fn format_compact_tokens(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn format_grouped(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (idx, ch) in s.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

fn format_local_time(iso: &str) -> String {
    if iso.is_empty() {
        return "-".to_string();
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(iso) {
        let local: DateTime<Local> = dt.with_timezone(&Local);
        return local.format("%m/%d %H:%M").to_string();
    }
    iso.chars().take(16).collect()
}

fn format_countdown(iso: &str) -> String {
    let Ok(dt) = DateTime::parse_from_rfc3339(iso) else {
        return String::new();
    };
    let diff = dt.with_timezone(&Utc).signed_duration_since(Utc::now());
    let secs = diff.num_seconds();
    if secs <= 0 {
        return "即將重置".to_string();
    }
    let mins = secs / 60;
    let hours = mins / 60;
    let days = hours / 24;
    if days > 0 {
        format!("↻ {}d {}h 後重置", days, hours % 24)
    } else if hours > 0 {
        format!("↻ {}h {}m 後重置", hours, mins % 60)
    } else {
        format!("↻ {}m 後重置", mins.max(1))
    }
}
