use sea_orm_migration::prelude::*;

mod m20260405_000001_init;
mod m20260406_000002_asr_subtitles;
mod m20260407_000003_media_tasks;
mod m20260407_000004_clip_batch;
mod m20260408_000005_workspace_clip_output;
mod m20260416_000006_add_missing_indexes;
mod m20260417_000007_video_missing;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260405_000001_init::Migration),
            Box::new(m20260406_000002_asr_subtitles::Migration),
            Box::new(m20260407_000003_media_tasks::Migration),
            Box::new(m20260407_000004_clip_batch::Migration),
            Box::new(m20260408_000005_workspace_clip_output::Migration),
            Box::new(m20260416_000006_add_missing_indexes::Migration),
            Box::new(m20260417_000007_video_missing::Migration),
        ]
    }
}
