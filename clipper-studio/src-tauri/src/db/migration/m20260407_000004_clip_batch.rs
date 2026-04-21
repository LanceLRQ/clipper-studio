use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260407_000004_clip_batch"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Add batch columns to clip_tasks for grouping batch clips
        db.execute_unprepared("ALTER TABLE clip_tasks ADD COLUMN batch_id TEXT")
            .await?;

        db.execute_unprepared("ALTER TABLE clip_tasks ADD COLUMN batch_title TEXT")
            .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_clip_tasks_batch ON clip_tasks(batch_id)",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite doesn't support DROP COLUMN before 3.35.0
        // Just leave the columns — they'll be ignored if unused
        let _ = manager;
        Ok(())
    }
}
