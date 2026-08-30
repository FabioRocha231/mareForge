//! PostgreSQL migration CLI used by the MareForge workspace.

use std::collections::HashSet;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use sqlx::migrate::{Migrate, Migrator};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use url::Url;

/// Reversible migrations are embedded into the binary.
static MIGRATOR: Migrator = sqlx::migrate!("../../../migrations");

#[derive(Debug, Parser)]
#[command(
    name = "mareforge-db-migrate",
    about = "Apply or inspect MareForge database migrations"
)]
pub struct Cli {
    /// PostgreSQL connection URL. Defaults to the DATABASE_URL environment variable.
    #[arg(long)]
    pub database_url: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Apply pending migrations.
    Up,
    /// Revert the most recent applied migration.
    Down,
    /// Revert the most recent applied migration, then apply it again.
    Redo,
    /// List embedded migrations and whether each is applied.
    Status,
}

pub async fn run(cli: Cli) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let database_url = resolve_database_url(cli.database_url.as_deref())?;
    validate_database_url(&database_url)?;
    let pool = connect(&database_url).await?;

    match cli.command {
        Command::Up => up(&pool).await?,
        Command::Down => down(&pool).await?,
        Command::Redo => redo(&pool).await?,
        Command::Status => status(&pool).await?,
    }

    Ok(())
}

fn resolve_database_url(flag: Option<&str>) -> Result<String> {
    match flag {
        Some(url) => Ok(url.to_owned()),
        None => std::env::var("DATABASE_URL")
            .context("DATABASE_URL is not set; export DATABASE_URL or pass --database-url"),
    }
}

fn validate_database_url(database_url: &str) -> Result<()> {
    let parsed = Url::parse(database_url).context("DATABASE_URL is not a valid URL")?;
    if !matches!(parsed.scheme(), "postgres" | "postgresql") {
        bail!("DATABASE_URL must use the postgres:// or postgresql:// scheme");
    }
    Ok(())
}

async fn connect(database_url: &str) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await
        .context("failed to connect to PostgreSQL")
}

async fn up(pool: &PgPool) -> Result<()> {
    MIGRATOR
        .run(pool)
        .await
        .context("failed to apply migrations")?;
    tracing::info!("migrations applied");
    Ok(())
}

async fn down(pool: &PgPool) -> Result<()> {
    let applied = applied_versions(pool).await?;
    let Some(&latest) = applied.iter().max() else {
        tracing::info!("no applied migrations to revert");
        return Ok(());
    };

    MIGRATOR
        .undo(pool, latest - 1)
        .await
        .context("failed to revert migration")?;
    tracing::info!("reverted migration {latest}");
    Ok(())
}

async fn redo(pool: &PgPool) -> Result<()> {
    let applied = applied_versions(pool).await?;
    if let Some(&latest) = applied.iter().max() {
        MIGRATOR
            .undo(pool, latest - 1)
            .await
            .context("failed to revert migration before redo")?;
    }

    MIGRATOR
        .run(pool)
        .await
        .context("failed to reapply migrations")?;
    tracing::info!("redo complete");
    Ok(())
}

async fn status(pool: &PgPool) -> Result<()> {
    let applied: HashSet<i64> = applied_versions(pool).await?.into_iter().collect();

    println!(
        "{:<20} {:<20} {:<8} status",
        "version", "description", "type"
    );
    let mut migrations: Vec<_> = MIGRATOR.iter().collect();
    migrations.sort_by_key(|migration| migration.migration_type.is_down_migration());

    for migration in migrations {
        let migration_type = if migration.migration_type.is_down_migration() {
            "down"
        } else {
            "up"
        };
        let status = if applied.contains(&migration.version) {
            "applied"
        } else {
            "pending"
        };
        println!(
            "{:<20} {:<20} {:<8} {}",
            migration.version, migration.description, migration_type, status
        );
    }

    Ok(())
}

async fn applied_versions(pool: &PgPool) -> Result<Vec<i64>> {
    let mut connection = pool
        .acquire()
        .await
        .context("failed to acquire database connection")?;
    connection
        .ensure_migrations_table()
        .await
        .context("failed to ensure migrations table")?;
    let applied = connection
        .list_applied_migrations()
        .await
        .context("failed to list applied migrations")?;
    Ok(applied
        .into_iter()
        .map(|migration| migration.version)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::validate_database_url;

    #[test]
    fn rejects_non_postgres_database_urls() {
        assert!(validate_database_url("mysql://localhost/db").is_err());
        assert!(validate_database_url("postgres://localhost/db").is_ok());
    }

    #[test]
    fn flag_overrides_database_url_environment() {
        std::env::set_var("DATABASE_URL", "postgres://env/db");
        let resolved = super::resolve_database_url(Some("postgres://flag/db")).unwrap();
        assert_eq!(resolved, "postgres://flag/db");
        std::env::remove_var("DATABASE_URL");
    }
}
