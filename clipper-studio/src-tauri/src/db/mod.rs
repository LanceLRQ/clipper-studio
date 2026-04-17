pub mod migration;
pub mod models;

use std::path::Path;

use sea_orm::{ConnectOptions, Database as SeaDatabase, DatabaseConnection};
use sea_orm_migration::MigratorTrait;

use crate::db::migration::Migrator;

/// Database wrapper for ClipperStudio
#[derive(Debug, Clone)]
pub struct Database {
    conn: DatabaseConnection,
}

impl Database {
    /// Connect to SQLite database, creating it if not exists
    pub async fn connect(db_path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

        let mut opts = ConnectOptions::new(&db_url);
        opts.sqlx_logging(false); // Disable sqlx verbose logging

        let conn = SeaDatabase::connect(opts).await?;

        // Enable WAL mode for concurrent read support
        sea_orm::ConnectionTrait::execute_unprepared(&conn, "PRAGMA journal_mode=WAL;").await?;

        // Enable foreign keys
        sea_orm::ConnectionTrait::execute_unprepared(&conn, "PRAGMA foreign_keys=ON;").await?;

        tracing::info!("SQLite connected: {}", db_path.display());

        Ok(Self { conn })
    }

    /// Run all pending migrations
    pub async fn run_migrations(&self) -> Result<(), Box<dyn std::error::Error>> {
        Migrator::up(&self.conn, None).await?;
        tracing::info!("Database migrations completed");
        Ok(())
    }

    /// Get a reference to the database connection
    pub fn conn(&self) -> &DatabaseConnection {
        &self.conn
    }
}
