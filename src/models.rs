use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeriodKind {
    Today,
    Week,
    Month,
    AllTime,
}

impl Default for PeriodKind {
    fn default() -> Self {
        Self::Today
    }
}

impl PeriodKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Today => "今日 Today",
            Self::Week => "近7日 7D",
            Self::Month => "本月 30D",
            Self::AllTime => "總計 All-Time",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameModePolicy {
    Auto,
    ForceSleep,
    Off,
}

impl Default for GameModePolicy {
    fn default() -> Self {
        Self::Auto
    }
}

impl GameModePolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "自動偵測遊戲",
            Self::ForceSleep => "強制休眠 (0 I/O)",
            Self::Off => "停用遊戲偵測",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub refresh_interval_mins: u64,
    pub hide_inactive_tools: bool,
    pub game_mode: GameModePolicy,
    pub custom_game_processes: Vec<String>,
    pub selected_period: PeriodKind,
    pub auto_update_upstream: bool,
    pub auto_update_hours: u64,
    pub last_upstream_check_epoch: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            refresh_interval_mins: 15,
            hide_inactive_tools: false,
            game_mode: GameModePolicy::Auto,
            custom_game_processes: vec![
                "ForzaHorizon5.exe".to_string(),
                "ForzaHorizon6.exe".to_string(),
                "bfv.exe".to_string(),
                "bf6.exe".to_string(),
                "Cyberpunk2077.exe".to_string(),
                "eldenring.exe".to_string(),
            ],
            selected_period: PeriodKind::Today,
            auto_update_upstream: true,
            auto_update_hours: 12,
            last_upstream_check_epoch: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Snapshot {
    pub schema_version: u32,
    pub updated_at: String,
    pub upstream_version: String,
    pub upstream_commit: String,
    pub periods: SnapshotPeriods,
    pub limits: Vec<LimitProvider>,
    pub recent_sessions: Vec<RecentSession>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SnapshotPeriods {
    pub today: PeriodData,
    pub week: PeriodData,
    pub month: PeriodData,
    pub all_time: PeriodData,
}

impl SnapshotPeriods {
    pub fn get(&self, period: PeriodKind) -> &PeriodData {
        match period {
            PeriodKind::Today => &self.today,
            PeriodKind::Week => &self.week,
            PeriodKind::Month => &self.month,
            PeriodKind::AllTime => &self.all_time,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PeriodData {
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
    pub timed_output_tokens: u64,
    pub timed_duration_ms: u64,
    pub tools: Vec<ToolUsage>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ToolUsage {
    pub id: String,
    pub label: String,
    pub status: String,
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
    pub cost_usd: f64,
    pub session_count: u64,
    pub last_used_at: String,
    pub models: Vec<ModelUsage>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelUsage {
    pub name: String,
    pub tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LimitProvider {
    pub provider: String,
    pub label: String,
    pub account_label: String,
    pub account_name: String,
    pub account_email: String,
    pub status: String,
    pub updated_at: Option<String>,
    pub balance_usd: Option<f64>,
    pub balance_currency: Option<String>,
    pub windows: Vec<LimitWindow>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LimitWindow {
    pub kind: String,
    pub label: String,
    pub used_percent: Option<f64>,
    pub remaining_percent: Option<f64>,
    pub resets_at: Option<String>,
    pub reset_description: String,
    pub detail: String,
    pub metric: Option<String>,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub remaining: Option<f64>,
    pub currency: Option<String>,
    pub show_meter: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RecentSession {
    pub client: String,
    pub session_id: String,
    pub title: String,
    pub project_label: String,
    pub latest_model: String,
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub last_used_at: String,
    pub context_tokens: u64,
    pub context_window: u64,
}
