use crate::persist::config::{take_notice, RecoveryNotice};

#[tauri::command]
pub fn take_config_recovery_notice() -> Option<RecoveryNotice> {
    take_notice()
}
