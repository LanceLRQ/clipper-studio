use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260407_000003_media_tasks"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // ========== media_tasks ==========
        // General-purpose task table for transcode, merge, and other media operations.
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS media_tasks (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                task_type     TEXT NOT NULL,
                video_ids     TEXT,
                output_path   TEXT,
                preset_id     INTEGER,
                status        TEXT NOT NULL DEFAULT 'pending',
                progress      REAL NOT NULL DEFAULT 0.0,
                error_message TEXT,
                options       TEXT,
                created_at    TEXT NOT NULL DEFAULT (datetime('now')),
                completed_at  TEXT
            )",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_media_tasks_type ON media_tasks(task_type)",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_media_tasks_status ON media_tasks(status)",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS media_tasks")
            .await?;
        Ok(())
    }
}
