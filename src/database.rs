use anyhow::Context;
use std::ops::Deref;
use std::path::Path;

pub struct Database {
    pool: sqlx::SqlitePool,
}

impl Database {
    fn create_db_file(path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::File::create_new(path)?;
        Ok(())
    }
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        let path = Path::new(url.strip_prefix("sqlite://").unwrap_or(url));
        if !path.exists() {
            Self::create_db_file(path).context("Failed to create database file")?;
        }
        let pool = sqlx::SqlitePool::connect(url).await?;
        sqlx::migrate!()
            .run(&pool)
            .await
            .context("Failed to run migrations")?;
        Ok(Self { pool })
    }
}

impl Deref for Database {
    type Target = sqlx::SqlitePool;
    fn deref(&self) -> &Self::Target {
        &self.pool
    }
}
