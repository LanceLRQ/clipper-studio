use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260406_000002_asr_subtitles"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // ========== asr_tasks ==========
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS asr_tasks (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                video_id        INTEGER NOT NULL REFERENCES videos(id),
                status          TEXT NOT NULL DEFAULT 'pending',
                progress        REAL NOT NULL DEFAULT 0.0,
                asr_provider_id TEXT NOT NULL DEFAULT 'local',
                remote_task_id  TEXT,
                error_message   TEXT,
                retry_count     INTEGER NOT NULL DEFAULT 0,
                language        TEXT DEFAULT 'zh',
                segment_count   INTEGER,
                created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                started_at      TEXT,
                completed_at    TEXT
            )",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_asr_tasks_video ON asr_tasks(video_id)",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_asr_tasks_status ON asr_tasks(status)",
        )
        .await?;

        // ========== subtitle_segments ==========
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS subtitle_segments (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                video_id    INTEGER NOT NULL REFERENCES videos(id),
                language    TEXT NOT NULL DEFAULT 'zh',
                start_ms    INTEGER NOT NULL,
                end_ms      INTEGER NOT NULL,
                text        TEXT NOT NULL,
                source      TEXT NOT NULL DEFAULT 'asr',
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_sub_video_time ON subtitle_segments(video_id, start_ms, end_ms)",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_sub_time ON subtitle_segments(start_ms, end_ms)",
        )
        .await?;

        // ========== subtitle_fts (FTS5 full-text search) ==========
        db.execute_unprepared(
            "CREATE VIRTUAL TABLE IF NOT EXISTS subtitle_fts USING fts5(
                text,
                content=subtitle_segments,
                content_rowid=id
            )",
        )
        .await?;

        // FTS5 triggers for automatic sync
        db.execute_unprepared(
            "CREATE TRIGGER IF NOT EXISTS subtitle_fts_insert AFTER INSERT ON subtitle_segments BEGIN
                INSERT INTO subtitle_fts(rowid, text) VALUES (new.id, new.text);
            END",
        )
        .await?;

        db.execute_unprepared(
            "CREATE TRIGGER IF NOT EXISTS subtitle_fts_delete BEFORE DELETE ON subtitle_segments BEGIN
                INSERT INTO subtitle_fts(subtitle_fts, rowid, text) VALUES('delete', old.id, old.text);
            END",
        )
        .await?;

        db.execute_unprepared(
            "CREATE TRIGGER IF NOT EXISTS subtitle_fts_update AFTER UPDATE ON subtitle_segments BEGIN
                INSERT INTO subtitle_fts(subtitle_fts, rowid, text) VALUES('delete', old.id, old.text);
                INSERT INTO subtitle_fts(rowid, text) VALUES (new.id, new.text);
            END",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        for trigger in &[
            "subtitle_fts_insert",
            "subtitle_fts_delete",
            "subtitle_fts_update",
        ] {
            db.execute_unprepared(&format!("DROP TRIGGER IF EXISTS {}", trigger))
                .await?;
        }

        for table in &["subtitle_fts", "subtitle_segments", "asr_tasks"] {
            db.execute_unprepared(&format!("DROP TABLE IF EXISTS {}", table))
                .await?;
        }

        Ok(())
    }
}
