use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

// ==================== Types ====================

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TagInfo {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTagRequest {
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTagRequest {
    pub id: i64,
    pub name: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetVideoTagsRequest {
    pub video_id: i64,
    pub tag_ids: Vec<i64>,
}

// ==================== Commands ====================

/// Create a new tag
#[tauri::command]
pub async fn create_tag(
    state: State<'_, AppState>,
    req: CreateTagRequest,
) -> Result<TagInfo, String> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err("标签名称不能为空".to_string());
    }

    let color_sql = req
        .color
        .as_deref()
        .map(|c| format!("'{}'", c.replace('\'', "''")))
        .unwrap_or_else(|| "NULL".to_string());

    let sql = format!(
        "INSERT INTO tags (name, color) VALUES ('{}', {})",
        name.replace('\'', "''"),
        color_sql,
    );

    sea_orm::ConnectionTrait::execute_unprepared(state.db.conn(), &sql)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                format!("标签 '{}' 已存在", name)
            } else {
                format!("创建标签失败: {}", e)
            }
        })?;

    // Query the inserted row
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT * FROM tags WHERE name = '{}'",
                name.replace('\'', "''")
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("创建后查询失败".to_string())?;

    let tag = row_to_tag_info(&row);
    tracing::info!("Tag created: {} (id={})", tag.name, tag.id);
    Ok(tag)
}

/// List all tags
#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Result<Vec<TagInfo>, String> {
    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT * FROM tags ORDER BY name ASC".to_string(),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows.iter().map(row_to_tag_info).collect())
}

/// Update an existing tag
#[tauri::command]
pub async fn update_tag(
    state: State<'_, AppState>,
    req: UpdateTagRequest,
) -> Result<TagInfo, String> {
    let mut sets: Vec<String> = Vec::new();

    if let Some(ref name) = req.name {
        let name = name.trim();
        if name.is_empty() {
            return Err("标签名称不能为空".to_string());
        }
        sets.push(format!("name = '{}'", name.replace('\'', "''")));
    }

    if let Some(ref color) = req.color {
        if color.is_empty() {
            sets.push("color = NULL".to_string());
        } else {
            sets.push(format!("color = '{}'", color.replace('\'', "''")));
        }
    }

    if sets.is_empty() {
        return Err("没有需要更新的字段".to_string());
    }

    let sql = format!("UPDATE tags SET {} WHERE id = {}", sets.join(", "), req.id,);

    sea_orm::ConnectionTrait::execute_unprepared(state.db.conn(), &sql)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                "标签名称已被使用".to_string()
            } else {
                format!("更新标签失败: {}", e)
            }
        })?;

    // Return updated tag
    let row = sea_orm::ConnectionTrait::query_one(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT * FROM tags WHERE id = {}", req.id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("标签不存在".to_string())?;

    Ok(row_to_tag_info(&row))
}

/// Delete a tag (also removes all video-tag associations)
#[tauri::command]
pub async fn delete_tag(state: State<'_, AppState>, tag_id: i64) -> Result<(), String> {
    // Delete associations first
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!("DELETE FROM video_tags WHERE tag_id = {}", tag_id),
    )
    .await
    .map_err(|e| e.to_string())?;

    // Delete the tag
    let result = sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!("DELETE FROM tags WHERE id = {}", tag_id),
    )
    .await
    .map_err(|e| e.to_string())?;

    if result.rows_affected() == 0 {
        return Err("标签不存在".to_string());
    }

    tracing::info!("Tag deleted: id={}", tag_id);
    Ok(())
}

/// Get all tags for a specific video
#[tauri::command]
pub async fn get_video_tags(
    state: State<'_, AppState>,
    video_id: i64,
) -> Result<Vec<TagInfo>, String> {
    let rows = sea_orm::ConnectionTrait::query_all(
        state.db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT t.* FROM tags t \
                 INNER JOIN video_tags vt ON t.id = vt.tag_id \
                 WHERE vt.video_id = {} \
                 ORDER BY t.name ASC",
                video_id
            ),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows.iter().map(row_to_tag_info).collect())
}

/// Set tags for a video (replace all existing associations)
#[tauri::command]
pub async fn set_video_tags(
    state: State<'_, AppState>,
    req: SetVideoTagsRequest,
) -> Result<Vec<TagInfo>, String> {
    // Delete existing associations
    sea_orm::ConnectionTrait::execute_unprepared(
        state.db.conn(),
        &format!("DELETE FROM video_tags WHERE video_id = {}", req.video_id),
    )
    .await
    .map_err(|e| e.to_string())?;

    // Insert new associations
    for tag_id in &req.tag_ids {
        sea_orm::ConnectionTrait::execute_unprepared(
            state.db.conn(),
            &format!(
                "INSERT OR IGNORE INTO video_tags (video_id, tag_id) VALUES ({}, {})",
                req.video_id, tag_id
            ),
        )
        .await
        .map_err(|e| e.to_string())?;
    }

    // Return the updated tag list
    get_video_tags(state, req.video_id).await
}

// ==================== Helpers ====================

fn row_to_tag_info(row: &sea_orm::QueryResult) -> TagInfo {
    TagInfo {
        id: row.try_get("", "id").unwrap_or(0),
        name: row.try_get("", "name").unwrap_or_default(),
        color: row.try_get::<String>("", "color").ok(),
    }
}
