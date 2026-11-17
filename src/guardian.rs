use crate::models::{AppConfig, GameModePolicy, Snapshot};
use eframe::egui;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::mpsc::Sender;
use std::time::UNIX_EPOCH;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const UPSTREAM_REPO_URL: &str = "https://github.com/Javis603/token-monitor.git";
const EMBEDDED_BRIDGE_CJS: &str = include_str!("../bridge/collect-filtered.cjs");

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LogFingerprint {
    pub file_count: u64,
    pub max_mtime_secs: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone)]
pub enum WorkerEvent {
    CollectFinished {
        ok: bool,
        message: String,
        snapshot: Option<Snapshot>,
    },
    SyncFinished {
        ok: bool,
        updated: bool,
        message: String,
    },
}

pub fn resolve_project_root() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors() {
            if ancestor.join("upstream").join(".git").exists()
                || ancestor.join("bridge").join("collect-filtered.cjs").exists()
            {
                return ancestor.to_path_buf();
            }
        }
        if let Some(dir) = exe.parent() {
            return dir.to_path_buf();
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors() {
            if ancestor.join("upstream").join(".git").exists()
                || ancestor.join("bridge").join("collect-filtered.cjs").exists()
            {
                return ancestor.to_path_buf();
            }
        }
        return cwd;
    }
    PathBuf::from(".")
}

pub fn ensure_embedded_bridge(root: &Path) -> std::io::Result<PathBuf> {
    let bridge_dir = root.join("bridge");
    fs::create_dir_all(&bridge_dir)?;
    let script_path = bridge_dir.join("collect-filtered.cjs");
    let up_to_date = fs::read_to_string(&script_path)
        .map(|existing| existing == EMBEDDED_BRIDGE_CJS)
        .unwrap_or(false);
    if !up_to_date {
        fs::write(&script_path, EMBEDDED_BRIDGE_CJS)?;
    }
    Ok(script_path)
}

pub fn snapshot_path(root: &Path) -> PathBuf {
    root.join("data").join("latest-snapshot.json")
}

pub fn config_path(root: &Path) -> PathBuf {
    root.join("data").join("config.json")
}

pub fn load_config(root: &Path) -> AppConfig {
    let path = config_path(root);
    if let Ok(text) = fs::read_to_string(&path) {
        if let Ok(cfg) = serde_json::from_str::<AppConfig>(&text) {
            return cfg;
        }
    }
    AppConfig::default()
}

pub fn save_config(root: &Path, config: &AppConfig) {
    let path = config_path(root);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = fs::write(path, json);
    }
}

pub fn load_snapshot(root: &Path) -> Option<Snapshot> {
    let path = snapshot_path(root);
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str::<Snapshot>(&text).ok()
}

/// Checks whether a fullscreen game or configured game executable is active.
/// Returns `Some(reason)` when background scanning should pause (0 Disk I/O).
pub fn active_game_reason(config: &AppConfig) -> Option<String> {
    match config.game_mode {
        GameModePolicy::ForceSleep => return Some("手動強制遊戲休眠".to_string()),
        GameModePolicy::Off => return None,
        GameModePolicy::Auto => {}
    }

    #[cfg(windows)]
    {
        if is_windows_fullscreen_busy() {
            return Some("偵測到全螢幕應用 / 遊戲".to_string());
        }
        if let Some(proc_name) = find_running_game_process(&config.custom_game_processes) {
            return Some(format!("偵測到遊戲進程: {proc_name}"));
        }
    }

    None
}

#[cfg(windows)]
fn is_windows_fullscreen_busy() -> bool {
    use windows_sys::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUERY_USER_NOTIFICATION_STATE, QUNS_BUSY,
        QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    };

    let mut state: QUERY_USER_NOTIFICATION_STATE = 0;
    let hr = unsafe { SHQueryUserNotificationState(&mut state) };
    if hr != 0 {
        return false;
    }
    state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_BUSY || state == QUNS_PRESENTATION_MODE
}

#[cfg(windows)]
fn find_running_game_process(targets: &[String]) -> Option<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    if targets.is_empty() {
        return None;
    }

    let normalized_targets: Vec<String> = targets
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if normalized_targets.is_empty() {
        return None;
    }

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }

    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

    let mut matched = None;
    if unsafe { Process32FirstW(snapshot, &mut entry) } != 0 {
        loop {
            let end = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
            let lower = name.to_ascii_lowercase();
            if normalized_targets.iter().any(|t| t == &lower) {
                matched = Some(name);
                break;
            }
            if unsafe { Process32NextW(snapshot, &mut entry) } == 0 {
                break;
            }
        }
    }

    unsafe {
        CloseHandle(snapshot);
    }
    matched
}

fn user_home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn watched_tool_roots() -> Vec<PathBuf> {
    let Some(home) = user_home_dir() else {
        return Vec::new();
    };
    vec![
        home.join(".claude").join("projects"),
        home.join(".codex").join("sessions"),
        home.join(".gemini").join("antigravity").join("conversations"),
        home.join(".local").join("share").join("opencode"),
        home.join(".lmstudio").join("server-logs"),
    ]
}

fn visit_shallow(dir: &Path, depth: u8, budget: &mut u64, fp: &mut LogFingerprint) {
    if depth == 0 || *budget == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if let Ok(mtime) = meta.modified() {
            if let Ok(secs) = mtime.duration_since(UNIX_EPOCH) {
                fp.max_mtime_secs = fp.max_mtime_secs.max(secs.as_secs());
            }
        }
        if meta.is_file() {
            fp.file_count += 1;
            fp.total_bytes = fp.total_bytes.saturating_add(meta.len());
        } else if meta.is_dir() {
            visit_shallow(&entry.path(), depth - 1, budget, fp);
        }
    }
}

/// Fast metadata-only fingerprint over the 5 local tool directories (<2ms, zero file content reads).
pub fn compute_watched_fingerprint() -> LogFingerprint {
    let mut fp = LogFingerprint::default();
    let mut budget = 2000;
    for root in watched_tool_roots() {
        if root.exists() {
            visit_shallow(&root, 3, &mut budget, &mut fp);
        }
    }
    fp
}

fn hidden_cmd(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

fn run_hidden_stdout(program: &str, args: &[&str], cwd: &Path) -> Result<String, String> {
    let out = hidden_cmd(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("無法執行 {program}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        let short: String = detail.chars().take(180).collect();
        Err(format!("{program} 失敗 ({:?}): {short}", out.status.code()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokscaleWin32Manifest {
    pub release_repo: String,
    pub release_tag: String,
    pub package_name: String,
    pub asset_name: String,
}

pub fn parse_tokscale_manifest_win32_x64(json_text: &str) -> Option<TokscaleWin32Manifest> {
    let v: serde_json::Value = serde_json::from_str(json_text).ok()?;
    let release_repo = v
        .get("releaseRepo")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("Javis603/tokscale")
        .to_string();
    let release_tag = v.get("releaseTag")?.as_str()?.to_string();
    let win32 = v.get("platforms")?.get("win32-x64")?;
    let package_name = win32
        .get("package")
        .and_then(|x| x.as_str())
        .unwrap_or("@tokscale/cli-win32-x64-msvc")
        .to_string();
    let asset_name = win32.get("asset")?.as_str()?.to_string();
    Some(TokscaleWin32Manifest {
        release_repo,
        release_tag,
        package_name,
        asset_name,
    })
}

fn ensure_vendored_tokscale_binary(upstream: &Path) -> Result<(), String> {
    let script = upstream.join("scripts").join("ensure-vendored-tokscale.js");
    if !script.exists() {
        return Ok(());
    }

    let first_attempt = hidden_cmd("node")
        .arg("scripts/ensure-vendored-tokscale.js")
        .current_dir(upstream)
        .output()
        .map_err(|e| format!("無法執行 node ensure-vendored-tokscale: {e}"))?;

    if first_attempt.status.success() {
        return Ok(());
    }

    // Fallback: download pinned release binary with curl.exe (resilient against Node fetch timeouts)
    let manifest_path = upstream.join("scripts").join("vendor").join("tokscale.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("讀取 tokscale.json 失敗: {e}"))?;
    let info = parse_tokscale_manifest_win32_x64(&manifest_text)
        .ok_or_else(|| "解析 tokscale.json 失敗".to_string())?;

    let mut bin_dir = upstream.join("node_modules");
    for seg in info.package_name.split('/') {
        bin_dir = bin_dir.join(seg);
    }
    bin_dir = bin_dir.join("bin");
    fs::create_dir_all(&bin_dir).map_err(|e| format!("建立 tokscale 目錄失敗: {e}"))?;

    let target_exe = bin_dir.join("tokscale.exe");
    let temp_exe = bin_dir.join("tokscale.exe.dl-tmp");
    let url = format!(
        "https://github.com/{}/releases/download/{}/{}",
        info.release_repo, info.release_tag, info.asset_name
    );

    let temp_str = temp_exe.to_string_lossy().to_string();
    let curl_out: Output = hidden_cmd("curl.exe")
        .args([
            "-L",
            "--fail",
            "--retry",
            "3",
            "--connect-timeout",
            "15",
            "--max-time",
            "120",
            "-o",
            &temp_str,
            &url,
        ])
        .current_dir(upstream)
        .output()
        .map_err(|e| format!("無法執行 curl.exe: {e}"))?;

    if !curl_out.status.success() {
        let _ = fs::remove_file(&temp_exe);
        let err = String::from_utf8_lossy(&curl_out.stderr);
        let short: String = err.chars().take(160).collect();
        return Err(format!("下載 tokscale.exe 失敗: {short}"));
    }

    if target_exe.exists() {
        let _ = fs::remove_file(&target_exe);
    }
    fs::rename(&temp_exe, &target_exe)
        .map_err(|e| format!("替換 tokscale.exe 失敗: {e}"))?;

    run_hidden_stdout("node", &["scripts/ensure-vendored-tokscale.js"], upstream)?;
    Ok(())
}

fn run_upstream_sync_pipeline(root: &Path) -> Result<(bool, String), String> {
    let upstream = root.join("upstream");
    let git_dir = upstream.join(".git");
    let mut freshly_cloned = false;
    let old_commit: String;
    let old_lock_hash: String;

    if !git_dir.exists() {
        if let Some(parent) = upstream.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let upstream_str = upstream.to_string_lossy().to_string();
        run_hidden_stdout(
            "git",
            &["clone", "--depth", "1", UPSTREAM_REPO_URL, &upstream_str],
            root,
        )?;
        freshly_cloned = true;
        old_commit = String::new();
        old_lock_hash = String::new();
    } else {
        old_commit = run_hidden_stdout("git", &["rev-parse", "--short", "HEAD"], &upstream)
            .unwrap_or_default();
        old_lock_hash =
            run_hidden_stdout("git", &["rev-parse", "HEAD:package-lock.json"], &upstream)
                .unwrap_or_default();

        run_hidden_stdout("git", &["fetch", "origin", "main", "--depth", "1"], &upstream)?;
        let fetch_short =
            run_hidden_stdout("git", &["rev-parse", "--short", "FETCH_HEAD"], &upstream)?;

        if fetch_short != old_commit {
            run_hidden_stdout("git", &["reset", "--hard", "FETCH_HEAD"], &upstream)?;
        }
    }

    let new_commit = run_hidden_stdout("git", &["rev-parse", "--short", "HEAD"], &upstream)
        .unwrap_or_else(|_| "unknown".to_string());
    let new_lock_hash =
        run_hidden_stdout("git", &["rev-parse", "HEAD:package-lock.json"], &upstream)
            .unwrap_or_default();

    let updated = freshly_cloned || (!old_commit.is_empty() && old_commit != new_commit);

    let needs_npm = !upstream.join("node_modules").exists()
        || (!old_lock_hash.is_empty() && old_lock_hash != new_lock_hash);
    if needs_npm {
        run_hidden_stdout("cmd.exe", &["/C", "npm ci --omit=dev"], &upstream)?;
    }

    if updated || needs_npm || !upstream.join("node_modules").join("@tokscale").exists() {
        ensure_vendored_tokscale_binary(&upstream)?;
    }

    let msg = if freshly_cloned {
        format!("已初始化作者監測庫 ({new_commit})")
    } else if updated {
        format!("已自動升級作者監測庫: {old_commit} -> {new_commit}")
    } else {
        format!("已是原作者最新版 ({new_commit})")
    };

    Ok((updated, msg))
}

pub fn spawn_collector_async(root: PathBuf, tx: Sender<WorkerEvent>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let script = match ensure_embedded_bridge(&root) {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.send(WorkerEvent::CollectFinished {
                    ok: false,
                    message: format!("寫入內嵌橋接腳本失敗: {e}"),
                    snapshot: None,
                });
                ctx.request_repaint();
                return;
            }
        };

        let out = hidden_cmd("node").arg(&script).current_dir(&root).output();

        let event = match out {
            Ok(out) if out.status.success() => {
                let snapshot = load_snapshot(&root);
                WorkerEvent::CollectFinished {
                    ok: snapshot.is_some(),
                    message: if snapshot.is_some() {
                        "數據更新完成".to_string()
                    } else {
                        "採集器完成，但無法解析快照檔案".to_string()
                    },
                    snapshot,
                }
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                let short_err: String = err.chars().take(180).collect();
                WorkerEvent::CollectFinished {
                    ok: false,
                    message: format!(
                        "採集失敗 (code {:?}): {}",
                        out.status.code(),
                        if short_err.is_empty() {
                            "未知錯誤"
                        } else {
                            &short_err
                        }
                    ),
                    snapshot: None,
                }
            }
            Err(e) => WorkerEvent::CollectFinished {
                ok: false,
                message: format!("無法啟動 Node 採集橋接器: {e}"),
                snapshot: None,
            },
        };

        let _ = tx.send(event);
        ctx.request_repaint();
    });
}

pub fn spawn_git_pull_async(root: PathBuf, tx: Sender<WorkerEvent>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let event = match run_upstream_sync_pipeline(&root) {
            Ok((updated, message)) => WorkerEvent::SyncFinished {
                ok: true,
                updated,
                message,
            },
            Err(err) => WorkerEvent::SyncFinished {
                ok: false,
                updated: false,
                message: format!("同步更新失敗: {err}"),
            },
        };

        let _ = tx.send(event);
        ctx.request_repaint();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_snapshot_json_if_present() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let snap_path = snapshot_path(&root);
        if snap_path.exists() {
            let snap = load_snapshot(&root).expect("snapshot should deserialize cleanly");
            assert_eq!(snap.schema_version, 1);
            assert_eq!(snap.periods.today.tools.len(), 7);
            assert_eq!(snap.periods.all_time.tools.len(), 7);
        }
    }

    #[test]
    fn game_mode_policies_behave_cleanly() {
        let mut cfg = AppConfig::default();
        cfg.game_mode = GameModePolicy::ForceSleep;
        assert!(active_game_reason(&cfg).is_some());

        cfg.game_mode = GameModePolicy::Off;
        assert!(active_game_reason(&cfg).is_none());

        cfg.game_mode = GameModePolicy::Auto;
        // Just confirm Auto mode runs Win32 checks without panicking
        let _ = active_game_reason(&cfg);
    }

    #[test]
    fn watched_fingerprint_runs_without_error() {
        let _fp = compute_watched_fingerprint();
    }

    #[test]
    fn parses_vendored_tokscale_manifest_and_embedded_bridge() {
        assert!(EMBEDDED_BRIDGE_CJS.contains("CANDIDATE_CLIENT_IDS"));

        let temp_dir = std::env::temp_dir().join("token_monitor_rs_bridge_test");
        let _ = fs::remove_dir_all(&temp_dir);
        let written = ensure_embedded_bridge(&temp_dir).expect("should write embedded bridge");
        assert!(written.exists());
        let read_back = fs::read_to_string(&written).expect("should read written bridge");
        assert_eq!(read_back, EMBEDDED_BRIDGE_CJS);
        let _ = fs::remove_dir_all(&temp_dir);

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let manifest_path = root.join("upstream").join("scripts").join("vendor").join("tokscale.json");
        if manifest_path.exists() {
            let txt = fs::read_to_string(manifest_path).unwrap();
            let parsed = parse_tokscale_manifest_win32_x64(&txt).expect("manifest parses");
            assert!(!parsed.release_tag.is_empty());
            assert!(!parsed.asset_name.is_empty());
        }
    }
}
