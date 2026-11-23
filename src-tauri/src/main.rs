// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;
use handy_app_lib::CliArgs;

fn main() {
    #[cfg(target_os = "linux")]
    configure_webkit_renderer();

    #[cfg(target_os = "windows")]
    {
        // Avoid overlay/capture layer crashes (#2049). Set before backend
        // initialization, preserving user overrides.
        if std::env::var_os("VK_LOADER_LAYERS_DISABLE").is_none()
            && !handy_app_lib::env_flag_enabled("HANDY_KEEP_VULKAN_IMPLICIT_LAYERS")
        {
            std::env::set_var("VK_LOADER_LAYERS_DISABLE", "~implicit~");
        }
    }

    if handy_app_lib::engine_supervisor::is_worker_invocation() {
        std::process::exit(handy_app_lib::engine_supervisor::run_worker());
    }

    let cli_args = CliArgs::parse();
    handy_app_lib::run(cli_args)
}

#[cfg(target_os = "linux")]
fn configure_webkit_renderer() {
    const DISABLE: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
    const FORCE_SHM: &str = "WEBKIT_DMABUF_RENDERER_FORCE_SHM";
    if std::env::var_os(DISABLE).is_some() || std::env::var_os(FORCE_SHM).is_some() {
        return;
    }

    // SAFETY: these functions read library version constants before GTK starts.
    let version = unsafe {
        (
            webkit2gtk_sys::webkit_get_major_version(),
            webkit2gtk_sys::webkit_get_minor_version(),
        )
    };
    // WebKitGTK 2.54 keeps undamaged pixels only on the composited path.
    // Shared memory still avoids DMA-BUF imports while painting transparent cards.
    std::env::set_var(
        if version >= (2, 54) {
            FORCE_SHM
        } else {
            DISABLE
        },
        "1",
    );
}
