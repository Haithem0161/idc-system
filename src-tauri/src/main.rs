// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    sanitize_snap_env();

    app_lib::run();
}

// Snap-packaged terminals (e.g. VS Code installed via snap) inject GTK/GLib
// module and loader paths that point inside the snap's runtime. WebKit's
// helper processes inherit them and crash resolving glibc-private symbols
// (WebKitNetworkProcess: __libc_pthread_init), so any var referencing a snap
// path must be dropped before the webview initializes.
#[cfg(target_os = "linux")]
fn sanitize_snap_env() {
    const SNAP_TAINTED_VARS: [&str; 8] = [
        "GTK_PATH",
        "GTK_EXE_PREFIX",
        "GTK_IM_MODULE_FILE",
        "GIO_MODULE_DIR",
        "GSETTINGS_SCHEMA_DIR",
        "GDK_PIXBUF_MODULE_FILE",
        "GDK_PIXBUF_MODULEDIR",
        "LOCPATH",
    ];
    // SAFETY: called at the top of main, before any other thread exists and
    // before the Tokio runtime or GTK initializes, so no concurrent access to
    // the process environment is possible.
    for var in SNAP_TAINTED_VARS {
        if std::env::var(var).is_ok_and(|value| value.contains("/snap/")) {
            unsafe { std::env::remove_var(var) };
        }
    }
    if let Ok(paths) = std::env::var("LD_LIBRARY_PATH") {
        let kept: Vec<&str> = paths
            .split(':')
            .filter(|path| !path.is_empty() && !path.contains("/snap/"))
            .collect();
        if kept.join(":") != paths {
            unsafe { std::env::set_var("LD_LIBRARY_PATH", kept.join(":")) };
        }
    }
}
