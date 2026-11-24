//! Linux-specific desktop integration before GTK startup and window placement.

use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::time::Duration;

use atspi::zbus::{
    proxy::{Builder, CacheProperties},
    Connection, Proxy,
};

/// Handle the AppImage's Recall helper mode and configure the renderer before GTK starts.
pub fn prepare_process() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() == Some(OsStr::new("--recall-mcp")) {
        let error = match std::env::current_exe() {
            Ok(executable) => {
                std::process::Command::new(executable.with_file_name("grain-recall-mcp"))
                    .args(args)
                    .exec()
            }
            Err(error) => error,
        };
        eprintln!("Could not start Grain Recall: {error}");
        std::process::exit(1);
    }

    restore_appimage_wayland_backend();

    const DISABLE: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
    const FORCE_SHM: &str = "WEBKIT_DMABUF_RENDERER_FORCE_SHM";
    if std::env::var_os(DISABLE).is_some() || std::env::var_os(FORCE_SHM).is_some() {
        return;
    }

    // SAFETY: these accessors read version constants from the linked library.
    let version = unsafe {
        (
            webkit2gtk_sys::webkit_get_major_version(),
            webkit2gtk_sys::webkit_get_minor_version(),
        )
    };
    // WebKitGTK 2.54 keeps undamaged pixels only on the composited path.
    // Shared memory bypasses DMA-BUF imports while painting transparent cards.
    std::env::set_var(
        if version >= (2, 54) {
            FORCE_SHM
        } else {
            DISABLE
        },
        "1",
    );
}

/// Only start hidden when a desktop host can make the GTK indicator reachable.
pub(crate) fn has_tray_host() -> bool {
    tauri::async_runtime::block_on(async {
        tokio::time::timeout(Duration::from_millis(250), async {
            let connection = Connection::session().await?;
            let watcher = Builder::<Proxy<'_>>::new(&connection)
                .destination("org.kde.StatusNotifierWatcher")?
                .interface("org.kde.StatusNotifierWatcher")?
                .path("/StatusNotifierWatcher")?
                .cache_properties(CacheProperties::No)
                .build()
                .await?;
            watcher
                .get_property::<bool>("IsStatusNotifierHostRegistered")
                .await
        })
        .await
        .is_ok_and(|result| result.unwrap_or(false))
    })
}

fn should_restore_wayland_backend(
    has_appimage: bool,
    has_wayland_display: bool,
    gdk_backend: Option<&OsStr>,
    keep_override: bool,
) -> bool {
    !keep_override && has_appimage && has_wayland_display && gdk_backend == Some(OsStr::new("x11"))
}

/// Tauri's `linuxdeploy-plugin-gtk.sh` AppImage hook unconditionally exports
/// `GDK_BACKEND=x11`, forcing GTK onto XWayland and disabling `gtk-layer-shell`
/// for the recording overlay and Agent card. Since Grain's AppImage excludes
/// host Wayland libraries, restore GTK's default `wayland,x11` order so
/// Wayland works natively with automatic X11 fallback.
fn restore_appimage_wayland_backend() {
    let gdk_backend = std::env::var_os("GDK_BACKEND");
    if should_restore_wayland_backend(
        std::env::var_os("APPIMAGE").is_some(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        gdk_backend.as_deref(),
        crate::utils::env_flag_enabled("GRAIN_KEEP_GDK_BACKEND"),
    ) {
        std::env::set_var("GDK_BACKEND", "wayland,x11");
    }
}

#[cfg(test)]
mod tests {
    use super::should_restore_wayland_backend;
    use std::ffi::OsStr;

    #[test]
    fn restores_wayland_backend_only_for_appimage_x11_override_on_wayland() {
        assert!(should_restore_wayland_backend(
            true,
            true,
            Some(OsStr::new("x11")),
            false,
        ));
        assert!(!should_restore_wayland_backend(
            false,
            true,
            Some(OsStr::new("x11")),
            false,
        ));
        assert!(!should_restore_wayland_backend(
            true,
            false,
            Some(OsStr::new("x11")),
            false,
        ));
        assert!(!should_restore_wayland_backend(
            true,
            true,
            Some(OsStr::new("wayland")),
            false,
        ));
        assert!(!should_restore_wayland_backend(
            true,
            true,
            Some(OsStr::new("x11")),
            true,
        ));
    }
}
