#![cfg(feature = "db-tests")]

use sqlx::PgPool;

#[sqlx::test(migrator = "gridforge_store::migrations::MIGRATOR")]
async fn migration_fresh_database_creates_schema(pool: PgPool) {
    let table: Option<String> = sqlx::query_scalar("select to_regclass('public.events')::text")
        .fetch_one(&pool)
        .await
        .expect("query events table");
    assert_eq!(table.as_deref(), Some("events"));

    let index: Option<String> =
        sqlx::query_scalar("select to_regclass('public.events_recent_idx')::text")
            .fetch_one(&pool)
            .await
            .expect("query events index");
    assert_eq!(index.as_deref(), Some("events_recent_idx"));

    let check: Option<String> = sqlx::query_scalar(
        "select conname from pg_constraint
         where conrelid = 'events'::regclass
           and conname = 'events_payload_size_matches'",
    )
    .fetch_optional(&pool)
    .await
    .expect("query payload check");
    assert_eq!(check.as_deref(), Some("events_payload_size_matches"));
}

#[sqlx::test(migrator = "gridforge_store::migrations::MIGRATOR")]
async fn migration_is_idempotent(pool: PgPool) {
    gridforge_store::migrations::MIGRATOR
        .run(&pool)
        .await
        .expect("second migration run");
}

#[sqlx::test(migrator = "gridforge_store::migrations::MIGRATOR")]
async fn events_table_has_org_id(pool: PgPool) {
    let column: Option<(String, String)> = sqlx::query_as(
        "select data_type, is_nullable
         from information_schema.columns
         where table_schema = 'public'
           and table_name = 'events'
           and column_name = 'org_id'",
    )
    .fetch_optional(&pool)
    .await
    .expect("query org_id column");

    assert_eq!(column, Some(("uuid".to_owned(), "NO".to_owned())));
}

#[sqlx::test(migrator = "gridforge_store::migrations::MIGRATOR")]
async fn migration_down_drops_events(pool: PgPool) {
    gridforge_store::migrations::MIGRATOR
        .undo(&pool, 0)
        .await
        .expect("revert migrations");

    let table: Option<String> = sqlx::query_scalar("select to_regclass('public.events')::text")
        .fetch_one(&pool)
        .await
        .expect("query events table");
    assert_eq!(table, None);
}
