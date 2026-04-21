use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260405_000001_init"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Execute raw SQL for full control over SQLite-specific features
        let db = manager.get_connection();

        // ========== workspaces ==========
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS workspaces (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                name           TEXT NOT NULL,
                path           TEXT NOT NULL UNIQUE,
                adapter_id     TEXT NOT NULL,
                adapter_config TEXT,
                auto_scan      BOOLEAN NOT NULL DEFAULT 1,
                created_at     TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .await?;

        // ========== streamers ==========
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS streamers (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                platform     TEXT NOT NULL DEFAULT 'unknown',
                room_id      TEXT,
                name         TEXT NOT NULL,
                avatar_url   TEXT,
                adapter_meta TEXT,
                UNIQUE(platform, room_id)
            )",
        )
        .await?;

        // ========== recording_sessions ==========
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS recording_sessions (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id INTEGER NOT NULL REFERENCES workspaces(id),
                streamer_id  INTEGER REFERENCES streamers(id),
                title        TEXT,
                started_at   TEXT,
                ended_at     TEXT,
                file_count   INTEGER NOT NULL DEFAULT 0,
                adapter_meta TEXT,
                created_at   TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .await?;

        // ========== videos ==========
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS videos (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path     TEXT NOT NULL UNIQUE,
                file_name     TEXT NOT NULL,
                file_hash     TEXT,
                file_size     INTEGER NOT NULL,
                duration_ms   INTEGER,
                width         INTEGER,
                height        INTEGER,
                has_subtitle  BOOLEAN NOT NULL DEFAULT 0,
                has_danmaku   BOOLEAN NOT NULL DEFAULT 0,
                has_envelope  BOOLEAN NOT NULL DEFAULT 0,
                workspace_id  INTEGER REFERENCES workspaces(id),
                streamer_id   INTEGER REFERENCES streamers(id),
                session_id    INTEGER REFERENCES recording_sessions(id),
                stream_title  TEXT,
                recorded_at   TEXT,
                adapter_id    TEXT,
                adapter_meta  TEXT,
                created_at    TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_videos_workspace ON videos(workspace_id)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_videos_streamer ON videos(streamer_id)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_videos_session ON videos(session_id)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_videos_recorded ON videos(recorded_at)",
        )
        .await?;

        // ========== audio_envelopes ==========
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS audio_envelopes (
                video_id    INTEGER PRIMARY KEY REFERENCES videos(id),
                window_ms   INTEGER NOT NULL,
                data        BLOB NOT NULL,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .await?;

        // ========== clip_tasks ==========
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS clip_tasks (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                video_id        INTEGER NOT NULL REFERENCES videos(id),
                start_time_ms   INTEGER NOT NULL,
                end_time_ms     INTEGER NOT NULL,
                title           TEXT,
                preset_id       INTEGER,
                status          TEXT NOT NULL DEFAULT 'pending',
                progress        REAL NOT NULL DEFAULT 0.0,
                error_message   TEXT,
                created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                completed_at    TEXT
            )",
        )
        .await?;

        // ========== clip_outputs ==========
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS clip_outputs (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                clip_task_id    INTEGER NOT NULL REFERENCES clip_tasks(id),
                video_id        INTEGER NOT NULL REFERENCES videos(id),
                output_path     TEXT NOT NULL,
                format          TEXT NOT NULL,
                variant         TEXT NOT NULL,
                file_size       INTEGER,
                include_danmaku BOOLEAN NOT NULL DEFAULT 0,
                include_subtitle BOOLEAN NOT NULL DEFAULT 0,
                created_at      TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .await?;

        // ========== encoding_presets ==========
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS encoding_presets (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT NOT NULL,
                category    TEXT NOT NULL DEFAULT 'general',
                base_preset TEXT,
                options     TEXT NOT NULL,
                is_builtin  BOOLEAN NOT NULL DEFAULT 0,
                sort_order  INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .await?;

        // ========== app_settings ==========
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS app_settings (
                id                  INTEGER PRIMARY KEY DEFAULT 1,
                theme               TEXT NOT NULL DEFAULT 'system',
                language            TEXT NOT NULL DEFAULT 'zh-CN',
                video_scan_dirs     TEXT,
                workspace_dir       TEXT,
                asr_mode            TEXT NOT NULL DEFAULT 'disabled',
                asr_local_path      TEXT,
                asr_remote_url      TEXT,
                asr_remote_api_key  TEXT,
                llm_provider_id     TEXT,
                llm_default_model   TEXT,
                default_output_format TEXT NOT NULL DEFAULT 'mp4',
                updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .await?;

        // ========== settings_kv ==========
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS settings_kv (
                key         TEXT PRIMARY KEY,
                value       TEXT NOT NULL,
                updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .await?;

        // ========== tags ==========
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS tags (
                id    INTEGER PRIMARY KEY AUTOINCREMENT,
                name  TEXT NOT NULL UNIQUE,
                color TEXT
            )",
        )
        .await?;

        // ========== video_tags ==========
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS video_tags (
                video_id INTEGER NOT NULL REFERENCES videos(id),
                tag_id   INTEGER NOT NULL REFERENCES tags(id),
                PRIMARY KEY (video_id, tag_id)
            )",
        )
        .await?;

        // ========== analytics_events ==========
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS analytics_events (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                event       TEXT NOT NULL,
                properties  TEXT,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_analytics_event ON analytics_events(event, created_at)",
        )
        .await?;

        // ========== Insert default settings ==========
        db.execute_unprepared("INSERT OR IGNORE INTO app_settings (id) VALUES (1)")
            .await?;

        // ========== Insert builtin encoding presets ==========
        db.execute_unprepared(
            r#"INSERT OR IGNORE INTO encoding_presets (name, category, options, is_builtin, sort_order) VALUES
                ('极速（无重编码）', 'general', '{"codec":"copy"}', 1, 1),
                ('标准质量', 'general', '{"codec":"auto","crf":23}', 1, 2),
                ('高质量', 'general', '{"codec":"auto","crf":18}', 1, 3),
                ('小文件', 'general', '{"codec":"h265","crf":28}', 1, 4),
                ('仅音频', 'general', '{"codec":"aac","audio_only":true}', 1, 5)
            "#
        ).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        let tables = [
            "analytics_events",
            "video_tags",
            "tags",
            "settings_kv",
            "app_settings",
            "encoding_presets",
            "clip_outputs",
            "clip_tasks",
            "audio_envelopes",
            "videos",
            "recording_sessions",
            "streamers",
            "workspaces",
        ];

        for table in tables {
            db.execute_unprepared(&format!("DROP TABLE IF EXISTS {}", table))
                .await?;
        }

        Ok(())
    }
}
