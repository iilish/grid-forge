use chrono::{DateTime, Utc};
use gridforge_domain::{NewEvent, OrgScope};
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::error::StoreError;

pub mod entity {
    use chrono::{DateTime, Utc};
    use sea_orm::entity::prelude::*;
    use uuid::Uuid;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "events")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        pub org_id: Uuid,
        pub device_id: String,
        pub boot_count: i64,
        pub seq: i64,
        pub schema_ver: i32,
        pub device_ts: DateTime<Utc>,
        pub ingest_ts: DateTime<Utc>,
        pub persisted_ts: DateTime<Utc>,
        pub payload: Vec<u8>,
        pub payload_bytes: i32,
        pub payload_type: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub use entity::{ActiveModel, Column, Entity, Model};

#[derive(Debug, Clone, FromRow)]
pub struct RecentEvent {
    pub id: Uuid,
    pub device_id: String,
    pub seq: i64,
    pub ingest_ts: DateTime<Utc>,
    pub payload: Vec<u8>,
    pub payload_type: String,
}

pub async fn insert_event(
    db: &DatabaseConnection,
    org: OrgScope,
    event: &NewEvent,
) -> Result<(), StoreError> {
    entity::ActiveModel {
        id: Set(event.id),
        org_id: Set(org.get()),
        device_id: Set(event.device_id.clone()),
        boot_count: Set(event.boot_count),
        seq: Set(event.seq),
        schema_ver: Set(event.schema_ver),
        device_ts: Set(event.device_ts),
        ingest_ts: Set(event.ingest_ts),
        persisted_ts: sea_orm::NotSet,
        payload: Set(event.payload.clone()),
        payload_bytes: Set(event.payload_bytes()),
        payload_type: Set(event.payload_type.clone()),
    }
    .insert(db)
    .await?;

    Ok(())
}

pub async fn recent_events(
    pool: &PgPool,
    org: OrgScope,
    limit: i64,
) -> Result<Vec<RecentEvent>, StoreError> {
    let rows = sqlx::query_as::<_, RecentEvent>(
        r#"
        SELECT id, device_id, seq, ingest_ts, payload, payload_type
        FROM events
        WHERE org_id = $1
        ORDER BY ingest_ts DESC, id DESC
        LIMIT $2
        "#,
    )
    .bind(org.get())
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Query)?;

    Ok(rows)
}
