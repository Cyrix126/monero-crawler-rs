use deadpool_diesel::sqlite::Pool;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
const MIGRATIONS: EmbeddedMigrations = embed_migrations!();
pub async fn run_migrations(pool: &Pool) -> anyhow::Result<()> {
    let conn = pool.get().await?;
    conn.interact(|conn| conn.run_pending_migrations(MIGRATIONS).map(|_| ()).unwrap())
        .await
        .unwrap();
    Ok(())
}
