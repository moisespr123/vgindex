pub mod models;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub const MAX_DB_CONNECTIONS: u32 = 30;

pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(MAX_DB_CONNECTIONS)
        .connect(database_url)
        .await
}

pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS _migrations (
            name VARCHAR(255) PRIMARY KEY,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    let applied: Vec<String> = sqlx::query_scalar("SELECT name FROM _migrations ORDER BY name")
        .fetch_all(pool)
        .await?;

    let mut entries: Vec<_> = std::fs::read_dir("migrations")
        .unwrap_or_else(|_| panic!("migrations directory not found"))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "sql"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if applied.contains(&name) {
            continue;
        }
        tracing::info!("Applying migration: {name}");
        let sql = std::fs::read_to_string(entry.path())
            .unwrap_or_else(|_| panic!("Failed to read migration {name}"));
        sqlx::raw_sql(&sql).execute(pool).await?;
        sqlx::query("INSERT INTO _migrations (name) VALUES ($1)")
            .bind(&name)
            .execute(pool)
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::MAX_DB_CONNECTIONS;

    const APACHE_MPM_CONFIG: &str = include_str!("../../docker/apache/mpm_prefork.conf");
    const MEDIAWIKI_DOCKERFILE: &str = include_str!("../../docker/mediawiki/Dockerfile");
    const PHPBB_DOCKERFILE: &str = include_str!("../../docker/phpbb/Dockerfile");
    const APACHE_MPM_COPY: &str =
        "COPY docker/apache/mpm_prefork.conf /etc/apache2/mods-available/mpm_prefork.conf";

    #[test]
    fn production_connection_budget_is_coordinated() {
        let max_request_workers = APACHE_MPM_CONFIG
            .lines()
            .find_map(|line| {
                line.split_whitespace()
                    .collect::<Vec<_>>()
                    .as_slice()
                    .strip_prefix(&["MaxRequestWorkers"])
                    .and_then(|values| values.first())
                    .and_then(|value| value.parse::<u32>().ok())
            })
            .expect("Apache MaxRequestWorkers must be configured");

        assert_eq!(MAX_DB_CONNECTIONS, 30);
        assert_eq!(max_request_workers, 30);
        assert_eq!(MAX_DB_CONNECTIONS + 2 * max_request_workers, 90);
        assert!(MEDIAWIKI_DOCKERFILE.contains(APACHE_MPM_COPY));
        assert!(PHPBB_DOCKERFILE.contains(APACHE_MPM_COPY));
    }
}
