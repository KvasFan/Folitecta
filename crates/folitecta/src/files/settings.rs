//! Settings exported to and imported from a file.

use super::*;

pub fn export_settings(st: AppState) {
    let json = serde_json::to_string_pretty(&*st.ui.read()).unwrap_or_default();
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("Settings", &["json"])
            .set_file_name("folitecta-settings.json")
            .save_file()
            .await
        else {
            return;
        };
        match folitecta_core::util::write_atomic(file.path(), json) {
            Ok(()) => st.toast(
                ToastKind::Ok,
                tf(
                    st.lang(),
                    "toast.saved",
                    &[("name", &file.path().display().to_string())],
                ),
                "",
            ),
            Err(e) => st.error(e),
        }
    });
}

pub fn import_settings(st: AppState) {
    spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("Settings", &["json"])
            .pick_file()
            .await
        else {
            return;
        };
        let text = String::from_utf8_lossy(&file.read().await).to_string();
        match serde_json::from_str::<crate::state::UiPrefs>(&text) {
            Ok(prefs) => {
                *st.ui_mut() = prefs;
                st.save_prefs();
                st.toast(ToastKind::Ok, t(st.lang(), "toast.settings_imported"), "");
            }
            Err(e) => st.error(e),
        }
    });
}
