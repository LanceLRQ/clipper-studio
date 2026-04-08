use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260408_000005_workspace_clip_output"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Add clip_output_dir column to workspaces table
        // NULL means use default logic (source file directory / clips/)
        db.execute_unprepared(
            "ALTER TABLE workspaces ADD COLUMN clip_output_dir TEXT"
        ).await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite does not support DROP COLUMN in older versions;
        // this migration is safe to leave as-is for rollback scenarios.
        Ok(())
    }
}
