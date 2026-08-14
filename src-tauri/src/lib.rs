mod clipboard;
mod db;
mod dedup;
mod models;
mod monitor;
mod state;
mod store;

use state::AppState;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// 极简日志器：输出到 stderr（tauri dev 会捕获）
struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        eprintln!("[PasteBoard][{}] {}", record.level(), record.args());
    }
    fn flush(&self) {}
}

fn init_logger() {
    let _ = log::set_logger(&SimpleLogger);
    log::set_max_level(log::LevelFilter::Info);
}

/// 切换主窗口显示/隐藏
fn toggle_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
        } else {
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
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
