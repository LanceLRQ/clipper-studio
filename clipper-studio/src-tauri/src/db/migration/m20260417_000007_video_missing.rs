use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260417_000007_video_missing"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 视频文件不在磁盘上时标记为 1，扫描时由 scan_workspace 维护
        db.execute_unprepared(
            "ALTER TABLE videos ADD COLUMN file_missing BOOLEAN NOT NULL DEFAULT 0",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
