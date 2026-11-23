use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("connection failed: {0}")]
    Connect(#[from] sqlx::Error),
    #[error("query failed: {0}")]
    Query(sqlx::Error),
    #[error("orm operation failed: {0}")]
    Orm(#[from] sea_orm::DbErr),
    #[error("migrations failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}
