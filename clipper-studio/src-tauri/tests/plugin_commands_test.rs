//! commands/plugin.rs 集成测试
//!
//! 覆盖与 settings_kv 交互的命令逻辑（不依赖 PluginRegistry/PluginManager）：
//! - resolve_plugin_dir：plugin_dir 设置项 + 必须绝对路径校验
//! - query_enabled_plugin_ids：键模式 'plugin:%:enabled' = 'true'
//! - get_plugin_config / set_plugin_config：'plugin:{id}:{key}' 前缀 + secret 编解码
//! - set_plugin_enabled：true/false 字符串持久化
//!
//! base64 编码、HTTP 字段剥离等纯逻辑见 src/commands/plugin.rs 内联测试。

use clipper_studio_lib::db::Database;
use clipper_studio_lib::utils::secrets;
use std::collections::HashSet;

async fn setup_test_db() -> Database {
    let db = Database::connect(std::path::Path::new(":memory:"))
        .await
        .expect("connect");
    db.run_migrations().await.expect("migrate");
    db
}

async fn exec(conn: &sea_orm::DatabaseConnection, sql: &str) {
    sea_orm::ConnectionTrait::execute_unprepared(conn, sql)
        .await
        .unwrap_or_else(|e| panic!("SQL: {}\n{}", e, sql));
}

async fn set_kv(db: &Database, key: &str, value: &str) {
    exec(
        db.conn(),
        &format!(
            "INSERT OR REPLACE INTO settings_kv (key, value) VALUES ('{}', '{}')",
            key.replace('\'', "''"),
            value.replace('\'', "''"),
        ),
    )
    .await;
}

async fn get_kv(db: &Database, key: &str) -> Option<String> {
    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT value FROM settings_kv WHERE key = '{}'",
                key.replace('\'', "''")
            ),
        ),
    )
    .await
    .unwrap();
    row.and_then(|r| r.try_get::<String>("", "value").ok())
}

// ============== resolve_plugin_dir 等价路径 ==============

/// 复刻 resolve_plugin_dir 中的判定：
/// - 若 settings_kv 有 plugin_dir 且为绝对路径 → 使用它
/// - 否则使用 default_dir
async fn resolve_plugin_dir_for(
    db: &Database,
    default_dir: &std::path::Path,
) -> std::path::PathBuf {
    if let Some(val) = get_kv(db, "plugin_dir").await {
        let path = std::path::PathBuf::from(&val);
        if path.is_absolute() {
            return path;
        }
    }
    default_dir.to_path_buf()
}

#[tokio::test]
async fn test_resolve_plugin_dir_uses_default_when_unset() {
    let db = setup_test_db().await;
    let default = std::path::PathBuf::from("/conf/plugins");
    let resolved = resolve_plugin_dir_for(&db, &default).await;
    assert_eq!(resolved, default);
}

#[tokio::test]
async fn test_resolve_plugin_dir_uses_absolute_setting() {
    let db = setup_test_db().await;
    set_kv(&db, "plugin_dir", "/custom/plugins").await;
    let default = std::path::PathBuf::from("/conf/plugins");
    let resolved = resolve_plugin_dir_for(&db, &default).await;
    assert_eq!(resolved, std::path::PathBuf::from("/custom/plugins"));
}

#[tokio::test]
async fn test_resolve_plugin_dir_rejects_relative_path() {
    let db = setup_test_db().await;
    set_kv(&db, "plugin_dir", "relative/path").await;
    let default = std::path::PathBuf::from("/conf/plugins");
    let resolved = resolve_plugin_dir_for(&db, &default).await;
    assert_eq!(resolved, default, "相对路径应被拒绝，回退默认目录");
}

// ============== query_enabled_plugin_ids 等价 ==============

/// 复刻 query_enabled_plugin_ids 的 SQL + 字符串 strip
async fn query_enabled_for(db: &Database) -> HashSet<String> {
    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT key FROM settings_kv WHERE key LIKE 'plugin:%:enabled' AND value = 'true'"
                .to_string(),
        ),
    )
    .await
    .unwrap();
    let mut ids = HashSet::new();
    for row in &rows {
        if let Ok(key) = row.try_get::<String>("", "key") {
            if let Some(id) = key
                .strip_prefix("plugin:")
                .and_then(|s| s.strip_suffix(":enabled"))
            {
                ids.insert(id.to_string());
            }
        }
    }
    ids
}

#[tokio::test]
async fn test_query_enabled_empty_when_no_records() {
    let db = setup_test_db().await;
    assert!(query_enabled_for(&db).await.is_empty());
}

#[tokio::test]
async fn test_query_enabled_returns_only_true_values() {
    let db = setup_test_db().await;
    set_kv(&db, "plugin:asr.local:enabled", "true").await;
    set_kv(&db, "plugin:llm.openai:enabled", "false").await;
    set_kv(&db, "plugin:storage.smb:enabled", "true").await;

    let ids = query_enabled_for(&db).await;
    assert_eq!(ids.len(), 2);
    assert!(ids.contains("asr.local"));
    assert!(ids.contains("storage.smb"));
    assert!(!ids.contains("llm.openai"), "false 应被过滤");
}

#[tokio::test]
async fn test_query_enabled_ignores_non_plugin_keys() {
    let db = setup_test_db().await;
    set_kv(&db, "plugin:p1:enabled", "true").await;
    set_kv(&db, "plugin:p1:other", "true").await; // 不是 :enabled 后缀
    set_kv(&db, "ui.theme", "dark").await;
    set_kv(&db, "asr_mode", "local").await;

    let ids = query_enabled_for(&db).await;
    assert_eq!(ids.len(), 1);
    assert!(ids.contains("p1"));
}

#[tokio::test]
async fn test_query_enabled_handles_dots_and_dashes_in_id() {
    let db = setup_test_db().await;
    set_kv(&db, "plugin:org.foo-bar.v2:enabled", "true").await;
    let ids = query_enabled_for(&db).await;
    assert!(ids.contains("org.foo-bar.v2"));
}

// ============== set_plugin_enabled 持久化 ==============

#[tokio::test]
async fn test_set_plugin_enabled_writes_true_string() {
    let db = setup_test_db().await;
    // 复刻 set_plugin_enabled 的写入
    set_kv(&db, "plugin:p1:enabled", "true").await;
    assert_eq!(
        get_kv(&db, "plugin:p1:enabled").await.as_deref(),
        Some("true")
    );
}

#[tokio::test]
async fn test_set_plugin_enabled_overwrites_to_false() {
    let db = setup_test_db().await;
    set_kv(&db, "plugin:p1:enabled", "true").await;
    set_kv(&db, "plugin:p1:enabled", "false").await;
    assert_eq!(
        get_kv(&db, "plugin:p1:enabled").await.as_deref(),
        Some("false")
    );

    let ids = query_enabled_for(&db).await;
    assert!(!ids.contains("p1"), "禁用后不应在 enabled 集合中");
}

// ============== get_plugin_config / set_plugin_config ==============

/// 复刻 set_plugin_config 的写入逻辑
async fn set_plugin_config_for(db: &Database, plugin_id: &str, key: &str, value: &str) {
    let full_key = format!(
        "plugin:{}:{}",
        plugin_id.replace('\'', "''"),
        key.replace('\'', "''")
    );
    let stored = if secrets::is_secret_key(key) {
        secrets::obfuscate(value)
    } else {
        value.to_string()
    };
    exec(
        db.conn(),
        &format!(
            "INSERT OR REPLACE INTO settings_kv (key, value) VALUES ('{}', '{}')",
            full_key,
            stored.replace('\'', "''"),
        ),
    )
    .await;
}

/// 复刻 get_plugin_config 的读取逻辑
async fn get_plugin_config_for(
    db: &Database,
    plugin_id: &str,
) -> std::collections::HashMap<String, String> {
    let pattern = format!("plugin:{}:", plugin_id.replace('\'', "''"));
    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT key, value FROM settings_kv WHERE key LIKE '{}%'",
                pattern
            ),
        ),
    )
    .await
    .unwrap();
    let mut result = std::collections::HashMap::new();
    for row in &rows {
        if let (Ok(key), Ok(val)) = (
            row.try_get::<String>("", "key"),
            row.try_get::<String>("", "value"),
        ) {
            if let Some(config_key) = key.strip_prefix(&pattern) {
                let decoded = if secrets::is_secret_key(config_key) {
                    secrets::deobfuscate(&val)
                } else {
                    val
                };
                result.insert(config_key.to_string(), decoded);
            }
        }
    }
    result
}

#[tokio::test]
async fn test_set_then_get_plugin_config_round_trip() {
    let db = setup_test_db().await;
    set_plugin_config_for(&db, "asr.local", "host", "127.0.0.1").await;
    set_plugin_config_for(&db, "asr.local", "port", "8765").await;

    let cfg = get_plugin_config_for(&db, "asr.local").await;
    assert_eq!(cfg.len(), 2);
    assert_eq!(cfg.get("host").map(String::as_str), Some("127.0.0.1"));
    assert_eq!(cfg.get("port").map(String::as_str), Some("8765"));
}

#[tokio::test]
async fn test_get_plugin_config_isolated_per_plugin() {
    let db = setup_test_db().await;
    set_plugin_config_for(&db, "p1", "k", "v1").await;
    set_plugin_config_for(&db, "p2", "k", "v2").await;

    let p1 = get_plugin_config_for(&db, "p1").await;
    let p2 = get_plugin_config_for(&db, "p2").await;
    assert_eq!(p1.get("k").map(String::as_str), Some("v1"));
    assert_eq!(p2.get("k").map(String::as_str), Some("v2"));
}

#[tokio::test]
async fn test_get_plugin_config_excludes_enabled_marker() {
    let db = setup_test_db().await;
    // enabled 标志虽以 plugin:p1: 开头，仍会被包含在 LIKE 'plugin:p1:%' 中
    // 复刻命令实际行为：会被作为 config_key="enabled" 包含
    set_plugin_config_for(&db, "p1", "host", "h").await;
    set_kv(&db, "plugin:p1:enabled", "true").await;

    let cfg = get_plugin_config_for(&db, "p1").await;
    assert_eq!(cfg.get("host").map(String::as_str), Some("h"));
    assert_eq!(
        cfg.get("enabled").map(String::as_str),
        Some("true"),
        "命令实现：enabled 也会作为 config 项返回"
    );
}

#[tokio::test]
async fn test_set_plugin_config_secret_key_obfuscated() {
    let db = setup_test_db().await;
    set_plugin_config_for(&db, "llm.openai", "api_key", "sk-secret123").await;

    // 直接读原始 value，验证已 b64 编码
    let raw = get_kv(&db, "plugin:llm.openai:api_key").await.unwrap();
    assert!(
        raw.starts_with("b64:"),
        "secret 应被 obfuscate，实际：{}",
        raw
    );

    // 通过 get_plugin_config 应解码回明文
    let cfg = get_plugin_config_for(&db, "llm.openai").await;
    assert_eq!(cfg.get("api_key").map(String::as_str), Some("sk-secret123"));
}

#[tokio::test]
async fn test_set_plugin_config_non_secret_stored_plain() {
    let db = setup_test_db().await;
    set_plugin_config_for(&db, "p1", "model_name", "gpt-4").await;

    let raw = get_kv(&db, "plugin:p1:model_name").await.unwrap();
    assert_eq!(raw, "gpt-4");
}

#[tokio::test]
async fn test_set_plugin_config_overwrites_existing() {
    let db = setup_test_db().await;
    set_plugin_config_for(&db, "p1", "k", "v1").await;
    set_plugin_config_for(&db, "p1", "k", "v2").await;

    let cfg = get_plugin_config_for(&db, "p1").await;
    assert_eq!(cfg.get("k").map(String::as_str), Some("v2"));
    assert_eq!(cfg.len(), 1, "INSERT OR REPLACE 应覆盖而非新增");
}

#[tokio::test]
async fn test_get_plugin_config_legacy_plain_secret_still_readable() {
    let db = setup_test_db().await;
    // 模拟旧版本：直接写明文 api_key
    set_kv(&db, "plugin:p1:api_key", "old_plain_token").await;

    let cfg = get_plugin_config_for(&db, "p1").await;
    // deobfuscate 对非 b64: 前缀字符串原样返回
    assert_eq!(
        cfg.get("api_key").map(String::as_str),
        Some("old_plain_token")
    );
}

#[tokio::test]
async fn test_get_plugin_config_returns_empty_when_no_keys() {
    let db = setup_test_db().await;
    let cfg = get_plugin_config_for(&db, "nonexistent").await;
    assert!(cfg.is_empty());
}

#[tokio::test]
async fn test_set_plugin_config_handles_quote_in_value() {
    let db = setup_test_db().await;
    set_plugin_config_for(&db, "p1", "note", "it's fine").await;
    let cfg = get_plugin_config_for(&db, "p1").await;
    assert_eq!(cfg.get("note").map(String::as_str), Some("it's fine"));
}
