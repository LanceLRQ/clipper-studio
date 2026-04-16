use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260416_000006_add_missing_indexes"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // clip_tasks: frequently queried by video_id, status, and both combined
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_clip_tasks_video_id ON clip_tasks(video_id)",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_clip_tasks_status ON clip_tasks(status)",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_clip_tasks_video_status ON clip_tasks(video_id, status)",
        )
        .await?;

        // clip_outputs: JOIN and CASCADE DELETE by clip_task_id and video_id
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_clip_outputs_clip_task_id ON clip_outputs(clip_task_id)",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_clip_outputs_video_id ON clip_outputs(video_id)",
        )
        .await?;

        // recording_sessions: filtered by workspace_id in list/delete operations
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_recording_sessions_workspace ON recording_sessions(workspace_id)",
        )
        .await?;

        // video_tags: tag-based filtering uses tag_id in subqueries
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_video_tags_tag_id ON video_tags(tag_id)",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Indexes are safe to leave in place
        Ok(())
    }
}
