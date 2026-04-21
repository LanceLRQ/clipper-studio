use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260417_000008_clip_task_burn_flags"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 烧录/导出开关持久化到 clip_tasks，保证失败/取消任务重试时参数不丢
        db.execute_unprepared(
            "ALTER TABLE clip_tasks ADD COLUMN include_danmaku BOOLEAN NOT NULL DEFAULT 0",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE clip_tasks ADD COLUMN include_subtitle BOOLEAN NOT NULL DEFAULT 0",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE clip_tasks ADD COLUMN export_subtitle BOOLEAN NOT NULL DEFAULT 0",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE clip_tasks ADD COLUMN export_danmaku BOOLEAN NOT NULL DEFAULT 0",
        )
        .await?;

        // 回填历史数据：已成功的任务从 clip_outputs 继承 include_* 状态
        db.execute_unprepared(
            "UPDATE clip_tasks SET \
                include_danmaku = COALESCE((SELECT co.include_danmaku FROM clip_outputs co WHERE co.clip_task_id = clip_tasks.id LIMIT 1), 0), \
                include_subtitle = COALESCE((SELECT co.include_subtitle FROM clip_outputs co WHERE co.clip_task_id = clip_tasks.id LIMIT 1), 0)",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
