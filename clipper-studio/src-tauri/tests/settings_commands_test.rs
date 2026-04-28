//! commands/system.rs settings 部分集成测试
//!
//! 验证 settings_kv 表 + secrets 模块的混淆逻辑配合 get/set/get_settings
//! 命令所执行 SQL 的端到端行为。

use clipper_studio_lib::db::Database;
use clipper_studio_lib::utils::secrets;

async fn setup_test_db() -> Database {
    let db = Database::connect(std::path::Path::new(":memory:"))
        .await
        .expect("failed to connect to in-memory SQLite");
    db.run_migrations().await.expect("failed to run migrations");
    db
}

/// 模拟 set_setting 命令所执行的 SQL（含 secret 编码逻辑）
async fn set_setting_sql(conn: &sea_orm::DatabaseConnection, key: &str, value: &str) {
    let stored = if secrets::is_secret_key(key) {
        secrets::obfuscate(value)
    } else {
        value.to_string()
    };
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "INSERT OR REPLACE INTO settings_kv (key, value) VALUES ('{}', '{}')",
            key.replace('\'', "''"),
            stored.replace('\'', "''"),
        ),
    )
    .await
    .expect("set_setting sql should succeed");
}

/// 模拟 get_setting 命令（含 secret 解码逻辑）
async fn get_setting_sql(conn: &sea_orm::DatabaseConnection, key: &str) -> Option<String> {
    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT value FROM settings_kv WHERE key = '{}'",
                key.replace('\'', "''")
            ),
        ),
    )
    .await
    .expect("get_setting query failed");

    row.and_then(|r| r.try_get::<String>("", "value").ok())
        .map(|v| {
            if secrets::is_secret_key(key) {
                secrets::deobfuscate(&v)
            } else {
                v
            }
        })
}

#[tokio::test]
async fn test_get_setting_missing_returns_none() {
    let db = setup_test_db().await;
    let result = get_setting_sql(db.conn(), "absent.key").await;
    assert!(result.is_none(), "未设置的 key 应返回 None");
}

#[tokio::test]
async fn test_set_then_get_plain_value() {
    let db = setup_test_db().await;
    set_setting_sql(db.conn(), "ui.theme", "dark").await;
    let got = get_setting_sql(db.conn(), "ui.theme").await;
    assert_eq!(got.as_deref(), Some("dark"));
}

#[tokio::test]
async fn test_set_overwrites_existing() {
    let db = setup_test_db().await;
    set_setting_sql(db.conn(), "ui.locale", "zh-CN").await;
    set_setting_sql(db.conn(), "ui.locale", "en-US").await;

    let got = get_setting_sql(db.conn(), "ui.locale").await;
    assert_eq!(
        got.as_deref(),
        Some("en-US"),
        "INSERT OR REPLACE 应覆盖旧值"
    );

    // 校验仍只有一行
    let count_row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT COUNT(*) AS cnt FROM settings_kv WHERE key = 'ui.locale'".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let cnt: i64 = count_row.try_get("", "cnt").unwrap();
    assert_eq!(cnt, 1);
}

#[tokio::test]
async fn test_secret_key_is_obfuscated_in_storage() {
    let db = setup_test_db().await;

    let secret = "sk-supersecretvalue123";
    set_setting_sql(db.conn(), "openai.api_key", secret).await;

    // 直接读原始 value，不走 deobfuscate，验证已编码
    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT value FROM settings_kv WHERE key = 'openai.api_key'".to_string(),
        ),
    )
    .await
    .unwrap()
    .expect("row");
    let raw: String = row.try_get("", "value").unwrap();

    assert!(
        raw.starts_with("b64:"),
        "secret 应以 b64: 前缀存储，实际：{}",
        raw
    );
    assert!(!raw.contains("supersecret"), "明文不应直接落库");

    // get_setting 应透明解码回明文
    let got = get_setting_sql(db.conn(), "openai.api_key").await;
    assert_eq!(got.as_deref(), Some(secret));
}

#[tokio::test]
async fn test_secret_key_detection_keywords() {
    let db = setup_test_db().await;

    for (key, val) in [
        ("user.password", "p@ss"),
        ("auth_token", "tok123"),
        ("plugin.basic_pass", "bp"),
        ("xx.secret", "sec"),
    ] {
        set_setting_sql(db.conn(), key, val).await;
        let got = get_setting_sql(db.conn(), key).await;
        assert_eq!(got.as_deref(), Some(val), "secret key {} 应可往返", key);

        // 原始值应是 b64: 前缀
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
        .unwrap()
        .unwrap();
        let raw: String = row.try_get("", "value").unwrap();
        assert!(raw.starts_with("b64:"), "{} 应被混淆", key);
    }
}

#[tokio::test]
async fn test_non_secret_key_stored_plain() {
    let db = setup_test_db().await;
    set_setting_sql(db.conn(), "feature.flag", "enabled").await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT value FROM settings_kv WHERE key = 'feature.flag'".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let raw: String = row.try_get("", "value").unwrap();
    assert_eq!(raw, "enabled", "普通 key 不应被编码");
}

#[tokio::test]
async fn test_legacy_plaintext_secret_still_readable() {
    let db = setup_test_db().await;

    // 模拟旧版本数据：直接写明文（不经 obfuscate）
    sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        "INSERT INTO settings_kv (key, value) VALUES ('legacy.token', 'old_plain_token')",
    )
    .await
    .unwrap();

    // get 应能兼容读出（deobfuscate 对非 b64: 前缀的字符串原样返回）
    let got = get_setting_sql(db.conn(), "legacy.token").await;
    assert_eq!(
        got.as_deref(),
        Some("old_plain_token"),
        "deobfuscate 应兼容旧明文"
    );
}

#[tokio::test]
async fn test_get_settings_batch_returns_subset() {
    let db = setup_test_db().await;

    set_setting_sql(db.conn(), "k1", "v1").await;
    set_setting_sql(db.conn(), "k2", "v2").await;
    set_setting_sql(db.conn(), "k3", "v3").await;

    // 模拟 get_settings：批量循环（注意：原命令未做 deobfuscate，对应这里仅取原始 value）
    let keys = vec!["k1".to_string(), "k3".to_string(), "missing".to_string()];
    let mut result = std::collections::HashMap::new();
    for key in &keys {
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

        if let Some(row) = row {
            if let Ok(val) = row.try_get::<String>("", "value") {
                result.insert(key.clone(), val);
            }
        }
    }

    assert_eq!(result.len(), 2, "missing 不应出现在结果中");
    assert_eq!(result.get("k1").map(String::as_str), Some("v1"));
    assert_eq!(result.get("k3").map(String::as_str), Some("v3"));
    assert!(!result.contains_key("missing"));
}

#[tokio::test]
async fn test_set_setting_with_quote_in_value() {
    let db = setup_test_db().await;
    // 单引号需经过 escape 防 SQL 注入 / 解析失败
    set_setting_sql(db.conn(), "user.note", "it's fine").await;
    let got = get_setting_sql(db.conn(), "user.note").await;
    assert_eq!(got.as_deref(), Some("it's fine"));
}

#[tokio::test]
async fn test_empty_secret_value_stored_as_empty_string() {
    let db = setup_test_db().await;
    // obfuscate("") 返回 ""，避免无意义的 b64: 占位符
    set_setting_sql(db.conn(), "api_key.x", "").await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT value FROM settings_kv WHERE key = 'api_key.x'".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let raw: String = row.try_get("", "value").unwrap();
    assert_eq!(raw, "", "空 secret 应存为空串而非 'b64:'");
}
