mod clipboard;
mod commands;
mod db;
mod dedup;
mod models;
mod monitor;
mod paste;
mod state;
mod store;

use state::AppState;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, PhysicalPosition, Position,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

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

/// 计算弹出窗位置：跟随鼠标，横向居中于光标，纵向在光标下方；
/// 自动钳制在光标所在显示器的工作区内
fn popup_position(win_w: i32, win_h: i32) -> (i32, i32) {
    unsafe {
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
        let x = (pt.x - win_w / 2).clamp(work.left + 8, work.right - win_w - 8);
        let y = (pt.y + 16).clamp(work.top + 8, work.bottom - win_h - 8);
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
            // 跟随鼠标定位
            let (x, y) = popup_position(420, 620);
            let _ = win.set_position(Position::Physical(PhysicalPosition::new(x, y)));
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
}

/// 构建系统托盘（显示/退出）
fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "显示 / 隐藏", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&toggle, &quit])?;

    TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => toggle_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// 注册全局热键：Ctrl+Shift+V 切换主窗口
fn setup_hotkey(app: &AppHandle) {
    if let Err(e) = app.global_shortcut().on_shortcut("Ctrl+Shift+V", |app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            toggle_main_window(app);
        }
    }) {
        eprintln!("failed to register global shortcut: {e}");
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
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            setup_tray(app.handle())?;
            setup_hotkey(app.handle());

            // 初始化数据目录与共享状态
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let state = AppState::new(data_dir.clone(), data_dir.join("pasteboard.db"))
                .expect("failed to init app state");
            log::info!("data dir: {}", data_dir.display());
            app.manage(state);

            // 启动剪贴板监听（事件驱动 + 轮询兜底）
            monitor::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_history,
            commands::pin_item,
            commands::delete_item,
            commands::clear_history,
            commands::get_settings,
            commands::set_max_items,
            commands::get_thumb,
            commands::get_image,
            commands::paste_item,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
