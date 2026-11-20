// Migrations are embedded at compile time. The future `gridforge migrate`
// command runs this migrator during deploy; the application never migrates on
// startup.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[cfg(test)]
mod tests {
    use sqlx::migrate::MigrationType;

    #[test]
    fn embeds_create_events_migration() {
        let migrations = &super::MIGRATOR.migrations;

        assert_eq!(migrations.len(), 2);
        assert_eq!(migrations[0].version, 1);
        assert_eq!(migrations[0].migration_type, MigrationType::ReversibleUp);
        assert_eq!(migrations[1].migration_type, MigrationType::ReversibleDown);
    }
}
