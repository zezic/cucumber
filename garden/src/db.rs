use anyhow::{Context, Result, anyhow};
use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use tracing::info;
use uuid::Uuid;

use crate::api::OauthInfo;

#[derive(Clone)]
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

    pub async fn get_user_by_token(&self, token: Uuid) -> Option<entity::user::Model> {
        todo!()
    }

    pub async fn create_user(&self, user_args: UserArgs) -> Result<i32> {
        todo!()
    }

    pub async fn login_user(&self, user_id: i32) -> Result<Uuid> {
        todo!()
    }

    pub async fn link_external_user(&self, info: OauthInfo, access_token: String, user_id: i32) -> Result<()> {
        todo!()
    }
}

pub struct UserArgs {
    pub username: String,
    pub display_name: String,
}