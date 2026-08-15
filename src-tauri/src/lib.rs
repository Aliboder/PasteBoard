mod clipboard;
mod commands;
mod db;
mod dedup;
mod file_icons;
mod models;
mod monitor;
mod paste;
mod state;
mod store;

use state::AppState;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
};

/// 托盘"开机自启"勾选项句柄（用于切换勾选状态）
static AUTOSTART_ITEM: std::sync::OnceLock<CheckMenuItem<tauri::Wry>> = std::sync::OnceLock::new();

/// 极简日志器：输出到 stderr（tauri dev 会捕获）+ 日志文件（%APPDATA%/com.aliboder.pasteboard/pasteboard.log）
struct SimpleLogger {
    file: std::sync::Mutex<std::fs::File>,
}

static LOGGER: std::sync::OnceLock<SimpleLogger> = std::sync::OnceLock::new();

impl log::Log for SimpleLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        let line = format!(
            "[{}] {}: {}",
            record.level(),
            record
                .module_path()
                .unwrap_or("pasteboard"),
            record.args()
        );
        eprintln!("{line}");
        if let Ok(mut file) = self.file.lock() {
            use std::io::Write;
            let _ = writeln!(file, "{line}");
        }
    }
    fn flush(&self) {}
}

fn init_logger() {
    // panic 也写入日志文件
    std::panic::set_hook(Box::new(|info| {
        eprintln!("[PasteBoard][PANIC] {info}");
    }));

    let default_path = format!(
        "{}/com.aliboder.pasteboard/pasteboard.log",
        std::env::var("APPDATA").unwrap_or_else(|_| ".".into())
    );
    let path = std::path::PathBuf::from(&default_path);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // 日志轮转：超过 1MB 时归档为 pasteboard.log.old（仅保留一份旧日志），防止长期运行无限增长
    const LOG_MAX_BYTES: u64 = 1024 * 1024;
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > LOG_MAX_BYTES {
            let old = path.with_extension("log.old");
            let _ = std::fs::remove_file(&old);
            let _ = std::fs::rename(&path, &old);
            log::info!("log rotated: {} -> {}", path.display(), old.display());
        }
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path);
    match file {
        Ok(file) => {
            if LOGGER.set(SimpleLogger { file: std::sync::Mutex::new(file) }).is_ok() {
                let _ = log::set_logger(LOGGER.get().unwrap());
                log::set_max_level(log::LevelFilter::Info);
                log::info!("pasteboard started, log file: {}", path.display());
            }
        }
        Err(e) => {
            eprintln!("[PasteBoard] failed to open log file {}: {e}", path.display());
        }
    }
}

/// 计算弹出窗位置：跟随鼠标（横向居中于光标、纵向在光标下方），
/// 全部使用 Win32 物理坐标（与 Tauri 的 DPI 换算无关），
/// 并钳制在光标所在显示器的工作区内，窄屏时防护
fn popup_position_physical(hwnd: windows::Win32::Foundation::HWND) -> (i32, i32) {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return (0, 0);
        }
        let win_w = rect.right - rect.left;
        let win_h = rect.bottom - rect.top;

        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_err() {
            return (0, 0); // 取不到光标则用系统默认位置
        }
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return (0, 0);
        }
        let work = info.rcWork;
        // 窗口大于工作区时避免 clamp 区间反转（clamp 要求 min<=max）
        let x = if work.right - work.left > win_w + 16 {
            (pt.x - win_w / 2).clamp(work.left + 8, work.right - win_w - 8)
        } else {
            work.left + 8
        };
        let y = if work.bottom - work.top > win_h + 16 {
            (pt.y + 16).clamp(work.top + 8, work.bottom - win_h - 8)
        } else {
            work.top + 8
        };
        (x, y)
    }
}

/// 切换主窗口显示/隐藏
fn toggle_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
        } else {
            // 唤起前记录前台窗口、焦点控件与选中范围，供粘贴回原窗口使用
            if let Some(state) = app.try_state::<AppState>() {
                let ctx = crate::paste::record_foreground();
                state
                    .prev_foreground
                    .store(ctx.hwnd, std::sync::atomic::Ordering::SeqCst);
                state
                    .prev_focus
                    .store(ctx.focus, std::sync::atomic::Ordering::SeqCst);
                state
                    .prev_sel_start
                    .store(ctx.sel_start, std::sync::atomic::Ordering::SeqCst);
                state
                    .prev_sel_end
                    .store(ctx.sel_end, std::sync::atomic::Ordering::SeqCst);
            }
            // 定位：跟随鼠标（可配置）或居中
            let follow_mouse = app
                .try_state::<AppState>()
                .and_then(|s| {
                    s.db
                        .lock()
                        .unwrap()
                        .get_setting("follow_mouse")
                        .ok()
                        .flatten()
                })
                .unwrap_or_else(|| "on".into());
            if follow_mouse == "on" {
                // 跟随鼠标定位：直接调用 Win32 API，避免 DPI 坐标换算偏差
                if let Ok(hwnd) = win.hwnd() {
                    let (x, y) = popup_position_physical(hwnd);
                    unsafe {
                        let _ = SetWindowPos(
                            hwnd,
                            None,
                            x,
                            y,
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
            } else {
                let _ = win.center();
            }
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
}

/// 构建系统托盘（显示/开机自启/退出）
fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "显示 / 隐藏", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "开机自启",
        true,
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;
    let _ = AUTOSTART_ITEM.set(autostart.clone());
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&toggle, &autostart, &quit])?;

    TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => toggle_main_window(app),
            "autostart" => {
                let enabled = app.autolaunch().is_enabled().unwrap_or(false);
                let result = if enabled {
                    app.autolaunch().disable()
                } else {
                    app.autolaunch().enable()
                };
                match result {
                    Ok(()) => {
                        let new_state = !enabled;
                        if let Some(item) = AUTOSTART_ITEM.get() {
                            let _ = item.set_checked(new_state);
                        }
                        log::info!("autostart set to {new_state}");
                    }
                    Err(e) => log::error!("failed to toggle autostart: {e}"),
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// 注册全局热键（先注销旧的）
fn register_hotkey(app: &AppHandle, hotkey: &str) -> bool {
    let _ = app.global_shortcut().unregister_all();
    match app.global_shortcut().register(hotkey) {
        Ok(_) => {
            log::info!("hotkey registered: {hotkey}");
            true
        }
        Err(e) => {
            log::error!("failed to register hotkey {hotkey}: {e}");
            false
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logger();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 已有实例运行时，唤起主窗口
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_main_window(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            // 1. 初始化数据目录与共享状态
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            crate::db::backup_database(&data_dir);
            let state = AppState::new(data_dir.clone(), data_dir.join("pasteboard.db"))
                .expect("failed to init app state");
            // 启动维护：数据库超过 8MB 时 VACUUM 回收碎片空间
            if let Ok(db) = state.db.lock() {
                let _ = db.vacuum_if_large(8 * 1024 * 1024);
            }
            log::info!("data dir: {}", data_dir.display());
            app.manage(state);

            // 2. 注册全局热键（使用已保存的自定义热键）
            let saved_hotkey = app
                .state::<AppState>()
                .db
                .lock()
                .unwrap()
                .get_setting("hotkey")
                .ok()
                .flatten()
                .unwrap_or_else(|| "Ctrl+Shift+V".into());
            register_hotkey(app.handle(), &saved_hotkey);

            // 3. 系统托盘
            setup_tray(app.handle())?;

            // 4. 启动剪贴板监听（事件驱动 + 轮询兜底）
            monitor::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_history,
            commands::pin_item,
            commands::delete_item,
            commands::clear_history,
            commands::clear_all_history,
            commands::get_settings,
            commands::set_max_items,
            commands::set_theme,
            commands::set_toggle,
            commands::set_autostart,
            commands::set_window_size,
            commands::open_data_dir,
            commands::get_data_dir,
            commands::get_stats,
            commands::reset_settings,
            commands::set_hotkey,
            commands::get_thumb,
            commands::get_image,
            commands::get_file_icon,
            commands::get_file_thumb,
            commands::get_file_preview,
            commands::paste_item,
            commands::copy_item,
            commands::open_file_location,
            commands::open_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
