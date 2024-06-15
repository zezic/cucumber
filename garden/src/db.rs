use anyhow::{Context, Result, anyhow};
use entity::sea_orm_active_enums::AuthProvider;
use entity::user;
use migration::{Migrator, MigratorTrait};
use sea_orm::IntoActiveModel;
use sea_orm::{ConnectOptions, Database, DatabaseConnection, EntityTrait, QueryFilter, Set};
use tracing::info;
use uuid::Uuid;
use sea_orm::ColumnTrait;
use sea_orm::ActiveModelTrait;

use crate::api::OauthInfo;

#[derive(Clone)]
pub struct Db {
    db: DatabaseConnection,
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

        Ok(Self { db: connection })
    }

    pub async fn get_user_by_token(&self, token: Uuid) -> Option<entity::user::Model> {
        let user = entity::user::Entity::find()
            .inner_join(entity::authenticated_client::Entity)
            .filter(entity::authenticated_client::Column::Token.eq(token))
            .one(&self.db)
            .await.ok().flatten();
        user
    }

    pub async fn create_user(&self, user_args: UserArgs) -> Result<i32> {
        const MAX_ATTEMPTS: usize = 50;
        for attempt in 0..MAX_ATTEMPTS {
            let user = entity::user::ActiveModel {
                username: Set(if attempt > 0 { format!("{}-{attempt}", user_args.username) } else { user_args.username.clone() }),
                display_name: Set(user_args.display_name.clone()),
                show_display_name: Set(false),
                ..Default::default()
            };
            if let Ok(user) = user.insert(&self.db).await {
                return Ok(user.id)
            }
        }
        return Err(anyhow!("username is occupied"))
    }

    pub async fn login_user(&self, user_id: i32) -> Result<Uuid> {
        let token = Uuid::new_v4();
        let authenticated_client = entity::authenticated_client::ActiveModel {
            user_id: Set(user_id),
            token: Set(token),
            ..Default::default()
        };
        authenticated_client.insert(&self.db).await?;
        Ok(token)
    }

    pub async fn link_external_user(&self, info: OauthInfo, provider: AuthProvider, access_token: String, user_id: i32) -> Result<()> {
        let existing_ext_user = entity::external_user::Entity::find()
            .filter(entity::external_user::Column::ExternalId.eq(info.external_id.clone()))
            .filter(entity::external_user::Column::AuthProvider.eq(provider.clone()))
            .one(&self.db).await.ok().flatten();
        if let Some(ext_user) = existing_ext_user {
            let mut ext_user = ext_user.into_active_model();
            ext_user.access_token = Set(access_token);
            ext_user.update(&self.db).await?;
        } else {
            let ext_user = entity::external_user::ActiveModel {
                user_id: Set(user_id),
                external_id: Set(info.external_id),
                auth_provider: Set(provider),
                data: Set(info.data),
                access_token: Set(access_token),
                ..Default::default()
            };
            ext_user.insert(&self.db).await?;
        }
        Ok(())
    }
}

pub struct UserArgs {
    pub username: String,
    pub display_name: String,
}