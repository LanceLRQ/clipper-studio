use sea_orm_migration::prelude::*;

mod m20260405_000001_init;
mod m20260406_000002_asr_subtitles;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260405_000001_init::Migration),
            Box::new(m20260406_000002_asr_subtitles::Migration),
        ]
    }
}
