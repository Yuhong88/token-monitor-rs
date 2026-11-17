use muda::{Menu, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub struct TrayController {
    pub _tray: TrayIcon,
    pub show_id: MenuId,
    pub refresh_id: MenuId,
    pub toggle_game_id: MenuId,
    pub sync_id: MenuId,
    pub quit_id: MenuId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    ShowWindow,
    RefreshNow,
    ToggleGameMode,
    SyncUpstream,
    Quit,
}

fn generate_rgba_icon() -> Icon {
    let size = 32usize;
    let mut rgba = vec![0u8; size * size * 4];

    for y in 0..size {
        for x in 0..size {
            let idx = (y * size + x) * 4;
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= 14.5 {
                // Smooth deep-indigo to emerald gradient circle
                let t = (x + y) as f32 / (2.0 * size as f32);
                let mut r = (24.0 + t * 14.0) as u8;
                let mut g = (144.0 + t * 50.0) as u8;
                let mut b = (205.0 - t * 45.0) as u8;

                // Stylized crisp "T" / bolt glyph inside the badge
                let in_top_bar = (8..=11).contains(&y) && (8..=23).contains(&x);
                let in_stem = (11..=23).contains(&y) && (14..=17).contains(&x);
                let in_dot = (20..=23).contains(&y) && (20..=23).contains(&x);

                if in_top_bar || in_stem {
                    r = 250;
                    g = 252;
                    b = 255;
                } else if in_dot {
                    r = 16;
                    g = 225;
                    b = 140;
                }

                rgba[idx] = r;
                rgba[idx + 1] = g;
                rgba[idx + 2] = b;
                rgba[idx + 3] = 255;
            }
        }
    }

    Icon::from_rgba(rgba, size as u32, size as u32).expect("valid 32x32 RGBA tray icon")
}

impl TrayController {
    pub fn build() -> Result<Self, String> {
        let menu = Menu::new();
        let show_item = MenuItem::new("📊 打開清爽看板", true, None);
        let refresh_item = MenuItem::new("🔄 立即更新數據", true, None);
        let toggle_game_item = MenuItem::new("🎮 切換遊戲休眠模式", true, None);
        let sync_item = MenuItem::new("⬆️ 同步原作者最新監測庫", true, None);
        let quit_item = MenuItem::new("❌ 完全退出", true, None);

        let show_id = show_item.id().clone();
        let refresh_id = refresh_item.id().clone();
        let toggle_game_id = toggle_game_item.id().clone();
        let sync_id = sync_item.id().clone();
        let quit_id = quit_item.id().clone();

        menu.append(&show_item).map_err(|e| e.to_string())?;
        menu.append(&refresh_item).map_err(|e| e.to_string())?;
        menu.append(&toggle_game_item).map_err(|e| e.to_string())?;
        menu.append(&sync_item).map_err(|e| e.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        menu.append(&quit_item).map_err(|e| e.to_string())?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Token Monitor RS — 輕量監控中")
            .with_icon(generate_rgba_icon())
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self {
            _tray: tray,
            show_id,
            refresh_id,
            toggle_game_id,
            sync_id,
            quit_id,
        })
    }

    pub fn update_tooltip(&self, text: &str) {
        let _ = self._tray.set_tooltip(Some(text));
    }

    #[allow(dead_code)]
    pub fn match_menu_id(&self, id: &MenuId) -> Option<TrayCommand> {
        if id == &self.show_id {
            Some(TrayCommand::ShowWindow)
        } else if id == &self.refresh_id {
            Some(TrayCommand::RefreshNow)
        } else if id == &self.toggle_game_id {
            Some(TrayCommand::ToggleGameMode)
        } else if id == &self.sync_id {
            Some(TrayCommand::SyncUpstream)
        } else if id == &self.quit_id {
            Some(TrayCommand::Quit)
        } else {
            None
        }
    }
}
