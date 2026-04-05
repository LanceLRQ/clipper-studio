use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

#[derive(Debug, Serialize)]
pub struct WorkspaceInfo {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub adapter_id: String,
    pub auto_scan: bool,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub path: String,
    /// Adapter type: "bililive-recorder", "generic", etc.
    pub adapter_id: String,
}

/// List all workspaces
#[tauri::command]
pub async fn list_workspaces(
    state: State<'_, AppState>,
) -> Result<Vec<WorkspaceInfo>, String> {
    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT id, name, path, adapter_id, auto_scan, created_at FROM workspaces ORDER BY created_at DESC"
                .to_string(),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let workspaces = rows
        .iter()
        .map(|row| {
            
            WorkspaceInfo {
                id: row.try_get("", "id").unwrap_or(0),
                name: row.try_get("", "name").unwrap_or_default(),
                path: row.try_get("", "path").unwrap_or_default(),
                adapter_id: row.try_get("", "adapter_id").unwrap_or_default(),
                auto_scan: row.try_get::<bool>("", "auto_scan").unwrap_or(true),
                created_at: row.try_get("", "created_at").unwrap_or_default(),
            }
        })
        .collect();

    Ok(workspaces)
}

/// Create a new workspace
#[tauri::command]
pub async fn create_workspace(
    state: State<'_, AppState>,
    req: CreateWorkspaceRequest,
) -> Result<WorkspaceInfo, String> {
    // Validate path exists
    let path = std::path::Path::new(&req.path);
    if !path.exists() {
        // Try to create the directory for new workspaces
        std::fs::create_dir_all(path).map_err(|e| format!("无法创建目录: {}", e))?;
    }
    if !path.is_dir() {
        return Err("指定路径不是一个目录".to_string());
    }

    // Insert into database
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!(
            "INSERT INTO workspaces (name, path, adapter_id) VALUES ('{}', '{}', '{}')",
            req.name.replace('\'', "''"),
            req.path.replace('\'', "''"),
            req.adapter_id.replace('\'', "''"),
        ),
    )
    .await
    .map_err(|e| format!("创建工作区失败: {}", e))?;

    // Get the created workspace
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT id, name, path, adapter_id, auto_scan, created_at FROM workspaces WHERE path = '{}'",
                req.path.replace('\'', "''")
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("创建后查询失败".to_string())?;

    
    let workspace = WorkspaceInfo {
        id: row.try_get("", "id").unwrap_or(0),
        name: row.try_get("", "name").unwrap_or_default(),
        path: row.try_get("", "path").unwrap_or_default(),
        adapter_id: row.try_get("", "adapter_id").unwrap_or_default(),
        auto_scan: row.try_get::<bool>("", "auto_scan").unwrap_or(true),
        created_at: row.try_get("", "created_at").unwrap_or_default(),
    };

    // Update config.toml recent workspaces
    if let Ok(mut config) = state.config.write() {
        config.add_recent_workspace(&req.path);
        let _ = config.save(&state.config_dir);
    }

    tracing::info!("Workspace created: {} ({})", workspace.name, workspace.path);
    Ok(workspace)
}

/// Delete a workspace (does not delete files on disk)
#[tauri::command]
pub async fn delete_workspace(
    state: State<'_, AppState>,
    workspace_id: i64,
) -> Result<(), String> {
    // Get workspace path before deletion (for config cleanup)
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT path FROM workspaces WHERE id = {}", workspace_id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    if let Some(row) = row {
        
        let path: String = row.try_get("", "path").unwrap_or_default();

        // Remove related data
        sea_orm::ConnectionTrait::execute_unprepared(
            state.db.conn(),
            &format!("DELETE FROM videos WHERE workspace_id = {}", workspace_id),
        )
        .await
        .map_err(|e| e.to_string())?;

        sea_orm::ConnectionTrait::execute_unprepared(
            state.db.conn(),
            &format!("DELETE FROM recording_sessions WHERE workspace_id = {}", workspace_id),
        )
        .await
        .map_err(|e| e.to_string())?;

        sea_orm::ConnectionTrait::execute_unprepared(
            state.db.conn(),
            &format!("DELETE FROM workspaces WHERE id = {}", workspace_id),
        )
        .await
        .map_err(|e| e.to_string())?;

        // Update config.toml
        if let Ok(mut config) = state.config.write() {
            config.remove_recent_workspace(&path);
            let _ = config.save(&state.config_dir);
        }

        tracing::info!("Workspace deleted: id={}", workspace_id);
    }

    Ok(())
}

/// Get active workspace ID from settings_kv
#[tauri::command]
pub async fn get_active_workspace(
    state: State<'_, AppState>,
) -> Result<Option<i64>, String> {
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT value FROM settings_kv WHERE key = 'active_workspace_id'".to_string(),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    match row {
        Some(row) => {
            
            let value: String = row.try_get("", "value").unwrap_or_default();
            Ok(value.parse::<i64>().ok())
        }
        None => Ok(None),
    }
}

/// Set active workspace ID
#[tauri::command]
pub async fn set_active_workspace(
    state: State<'_, AppState>,
    workspace_id: Option<i64>,
) -> Result<(), String> {
    match workspace_id {
        Some(id) => {
            sea_orm::ConnectionTrait::execute_unprepared(
                state.db.conn(),
                &format!(
                    "INSERT OR REPLACE INTO settings_kv (key, value) VALUES ('active_workspace_id', '{}')",
                    id
                ),
            )
            .await
            .map_err(|e| e.to_string())?;
        }
        None => {
            sea_orm::ConnectionTrait::execute_unprepared(
                state.db.conn(),
                "DELETE FROM settings_kv WHERE key = 'active_workspace_id'",
            )
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
