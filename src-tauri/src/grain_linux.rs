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
