//! Following the system's light/dark preference live.
//!
//! On Linux the source of truth is the XDG desktop portal (what GNOME, KDE and others set,
//! and what a Flatpak can always reach). Each change is applied to the window (native
//! decorations and WebKitGTK's `prefers-color-scheme` follow it) and sent to the UI as a
//! `system-theme` event. Elsewhere the webview's own `prefers-color-scheme` is used.

#[cfg(target_os = "linux")]
use tauri::{Emitter, Manager};
use tauri::{AppHandle, Runtime};

#[cfg(target_os = "linux")]
pub fn watch<R: Runtime>(app: AppHandle<R>) {
    use ashpd::desktop::settings::{ColorScheme, Settings};
    use futures_util::StreamExt;

    fn apply<R: Runtime>(app: &AppHandle<R>, scheme: ColorScheme) {
        let (theme, name) = match scheme {
            ColorScheme::PreferDark => (Some(tauri::Theme::Dark), "dark"),
            ColorScheme::PreferLight => (Some(tauri::Theme::Light), "light"),
            // "No preference" is light on every desktop that has the setting.
            ColorScheme::NoPreference => (Some(tauri::Theme::Light), "light"),
        };
        for w in app.webview_windows().values() {
            w.set_theme(theme).ok();
        }
        *app.state::<Current>().0.lock().unwrap() = Some(name);
        app.emit("system-theme", name).ok();
    }

    tauri::async_runtime::spawn(async move {
        let settings = match Settings::new().await {
            Ok(s) => s,
            Err(e) => {
                log::info!("no settings portal, following the webview's theme: {e}");
                return;
            }
        };
        if let Ok(s) = settings.color_scheme().await {
            apply(&app, s);
        }
        let Ok(mut changes) = settings.receive_color_scheme_changed().await else { return };
        while let Some(s) = changes.next().await {
            apply(&app, s);
        }
    });
}

#[cfg(not(target_os = "linux"))]
pub fn watch<R: Runtime>(_app: AppHandle<R>) {}

/// The last scheme the portal reported, for a UI that starts after the first event.
#[derive(Default)]
pub struct Current(pub std::sync::Mutex<Option<&'static str>>);

#[tauri::command]
pub fn system_theme(state: tauri::State<'_, Current>) -> Option<&'static str> {
    *state.0.lock().unwrap()
}
