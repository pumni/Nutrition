use std::env;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    let database_url =
        env::var("DATABASE_URL").expect("DATABASE_URL is required for the worker process");
    let pool = persistence_postgres::connect(&database_url, 4)
        .await
        .expect("worker could not connect to PostgreSQL");
    if env::var("RUN_MIGRATIONS").as_deref() == Ok("true") {
        persistence_postgres::migrate(&pool)
            .await
            .expect("database migration failed");
        info!("database migrations applied");
    }
    sqlx_healthcheck(&pool).await;
    info!("worker foundation process is ready; job claim loop is not enabled yet");
}

async fn sqlx_healthcheck(pool: &sqlx::PgPool) {
    sqlx::query("SELECT 1")
        .execute(pool)
        .await
        .expect("database health check failed");
}
