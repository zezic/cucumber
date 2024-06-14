use anyhow::{Context, Result, anyhow};
use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use tracing::info;

pub struct Db {
    connection: DatabaseConnection,
}

impl Db {
    pub async fn new(url: &str) -> Result<Self> {
        let mut opt = ConnectOptions::new(url.to_owned());
        opt.sqlx_logging(false); // Disable SQLx log
                                 // opt.sqlx_logging(true); // Enable SQLx log

        let connection = Database::connect(opt)
            .await
            .context("Failed to connect to database")?;
        match connection.ping().await {
            Ok(()) => info!("Successfully connected to database"),
            Err(err) => Err(anyhow!("Error connecting to database: {}", err))?,
        }

        Migrator::up(&connection, None)
            .await
            .context("Error running migration")?;

        Ok(Self { connection })
    }
}