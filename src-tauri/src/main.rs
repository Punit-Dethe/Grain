// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;
use handy_app_lib::CliArgs;

fn main() {
    // [GRAIN] Multicall architecture: if launched with --pill, run the overlay
    // logic and exit immediately. This avoids Tauri/winit event loop conflicts
    // while keeping the process visually unified in Task Manager.
    if std::env::args().any(|arg| arg == "--pill") {
        return grain_pill::run_pill();
    }

    let cli_args = CliArgs::parse();

    #[cfg(target_os = "linux")]
    {
        // DMABUF renderer causes crashes on various GPU/display server configurations
        // See: https://github.com/tauri-apps/tauri/issues/9394
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    handy_app_lib::run(cli_args)
}
