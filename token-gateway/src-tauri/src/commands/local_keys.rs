//! 本地 API Key 管理命令（原 `commands.rs` 的「Key」段）。

use tauri::State;

use crate::db::DbState;
use crate::keys;

// ---------------------------------------------------------------- Key ---

#[tauri::command]
pub fn keys_list(db: State<DbState>) -> Result<Vec<keys::LocalKey>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    keys::list_keys(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn key_create(
    db: State<DbState>,
    input: keys::KeyInput,
) -> Result<keys::KeyCreated, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    keys::create_key(&conn, &input)
}

#[tauri::command]
pub fn key_update(
    db: State<DbState>,
    id: i64,
    input: keys::KeyInput,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    keys::update_key(&conn, id, &input)
}

#[tauri::command]
pub fn key_set_enabled(
    db: State<DbState>,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    keys::set_key_enabled(&conn, id, enabled)
}

#[tauri::command]
pub fn key_delete(db: State<DbState>, id: i64) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    keys::delete_key(&conn, id)
}

#[tauri::command]
pub fn key_rotate(db: State<DbState>, id: i64) -> Result<keys::KeyCreated, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    keys::rotate_key(&conn, id)
}

#[tauri::command]
pub fn key_reveal(db: State<DbState>, id: i64) -> Result<String, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    keys::reveal_key(&conn, id)
}
