use std::time::Duration;

use gridforge_domain::{NewEvent, OrgScope};
use sea_orm::DatabaseConnection;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::error::StoreError;
use crate::events::{self, RecentEvent};
use crate::migrations::MIGRATOR;

const SERVE_MAX_CONNECTIONS: u32 = 5;
const SERVE_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(2);
const MIGRATE_MAX_CONNECTIONS: u32 = 1;

#[derive(Clone)]
pub struct Db {
    pool: PgPool,
    orm: DatabaseConnection,
}

impl Db {
    fn from_pool(pool: PgPool) -> Self {
        let orm = DatabaseConnection::from(pool.clone());
        Self { pool, orm }
    }

    pub async fn lazy(database_url: &str) -> Result<Self, StoreError> {
        let pool = PgPoolOptions::new()
            .max_connections(SERVE_MAX_CONNECTIONS)
            .acquire_timeout(SERVE_ACQUIRE_TIMEOUT)
            .connect_lazy(database_url)
            .map_err(StoreError::Connect)?;

        Ok(Self::from_pool(pool))
    }

    pub async fn connect(database_url: &str) -> Result<Self, StoreError> {
        let pool = PgPoolOptions::new()
            .max_connections(MIGRATE_MAX_CONNECTIONS)
            .connect(database_url)
            .await?;

        Ok(Self::from_pool(pool))
    }

    pub async fn migrate(&self) -> Result<(), StoreError> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }

    pub async fn probe(&self) -> bool {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .is_ok()
    }

    pub async fn insert_event(&self, org: OrgScope, event: &NewEvent) -> Result<(), StoreError> {
        events::insert_event(&self.orm, org, event).await
    }

    pub async fn recent_events(
        &self,
        org: OrgScope,
        limit: i64,
    ) -> Result<Vec<RecentEvent>, StoreError> {
        events::recent_events(&self.pool, org, limit).await
    }
}
