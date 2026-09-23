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

    // [GRAIN] `--eval <golden.json>` (Extensions V1 P3) is a headless subcommand
    // clap does not know about. Detect it before parsing so the upstream CliArgs
    // stays byte-identical; `run` reads the flag again to branch into eval.
    let cli_args = if handy_app_lib::eval_requested() {
        CliArgs::default()
    } else {
        CliArgs::parse()
    };

    #[cfg(target_os = "linux")]
    {
        // DMABUF renderer causes crashes on various GPU/display server configurations
        // See: https://github.com/tauri-apps/tauri/issues/9394
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    #[cfg(target_os = "windows")]
    {
        // Disable implicit Vulkan layers before the backend initializes; preserve
        // explicit loader settings and the user's opt-out.
        let keep_layers = std::env::var("HANDY_KEEP_VULKAN_IMPLICIT_LAYERS")
            .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        if std::env::var_os("VK_LOADER_LAYERS_DISABLE").is_none() && !keep_layers {
            std::env::set_var("VK_LOADER_LAYERS_DISABLE", "~implicit~");
        }
    }

    handy_app_lib::run(cli_args)
}
