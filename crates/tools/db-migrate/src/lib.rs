//! PostgreSQL migration CLI used by the Marvyr workspace.

use std::collections::HashSet;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use marvyr_domain_ships::{cosmetic_code, COSMETICS};
use sqlx::migrate::{Migrate, Migrator};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use url::Url;

/// Reversible migrations are embedded into the binary.
static MIGRATOR: Migrator = sqlx::migrate!("../../../migrations");

#[derive(Debug, Parser)]
#[command(
    name = "marvyr-db-migrate",
    about = "Apply or inspect Marvyr database migrations"
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
    /// Grant a cosmetic (appearance only) to a captain, by login name.
    GrantCosmetic {
        #[arg(long)]
        captain: String,
        #[arg(long)]
        cosmetic: String,
        /// Who granted it (audit trail).
        #[arg(long, default_value = "admin")]
        by: String,
    },
    /// Take a cosmetic back; if the captain was wearing it, the ship goes
    /// back to its default look.
    RevokeCosmetic {
        #[arg(long)]
        captain: String,
        #[arg(long)]
        cosmetic: String,
    },
    /// List the cosmetic catalog, or what one captain owns.
    ListCosmetics {
        #[arg(long)]
        captain: Option<String>,
    },
}

pub async fn run(cli: Cli) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // The catalog needs no database.
    if let Command::ListCosmetics { captain: None } = cli.command {
        for cosmetic in COSMETICS {
            println!("{:<16} {:?}\t{}", cosmetic.id, cosmetic.slot, cosmetic.name);
        }
        return Ok(());
    }

    let database_url = resolve_database_url(cli.database_url.as_deref())?;
    validate_database_url(&database_url)?;
    let pool = connect(&database_url).await?;

    match cli.command {
        Command::Up => up(&pool).await?,
        Command::Down => down(&pool).await?,
        Command::Redo => redo(&pool).await?,
        Command::Status => status(&pool).await?,
        Command::GrantCosmetic {
            captain,
            cosmetic,
            by,
        } => grant_cosmetic(&pool, &captain, &cosmetic, &by).await?,
        Command::RevokeCosmetic { captain, cosmetic } => {
            revoke_cosmetic(&pool, &captain, &cosmetic).await?
        }
        Command::ListCosmetics {
            captain: Some(captain),
        } => list_cosmetics(&pool, &captain).await?,
        // Catálogo: já listado antes de conectar.
        Command::ListCosmetics { captain: None } => {}
    }

    Ok(())
}

fn known_cosmetic(id: &str) -> Result<()> {
    if cosmetic_code(id).is_none() {
        bail!("unknown cosmetic `{id}` (see `list-cosmetics`)");
    }
    Ok(())
}

/// The captain's character (login name is case-insensitive, like the
/// `idx_accounts_username_lower` index). None = never set sail.
async fn captain_character(pool: &PgPool, captain: &str) -> Result<sqlx::types::Uuid> {
    let row: Option<(sqlx::types::Uuid,)> = sqlx::query_as(
        "SELECT c.id FROM characters c JOIN accounts a ON a.id = c.account_id \
         WHERE lower(a.username) = lower($1) ORDER BY c.last_seen_at DESC LIMIT 1",
    )
    .bind(captain)
    .fetch_optional(pool)
    .await
    .context("failed to look up the captain")?;
    row.map(|(id,)| id)
        .with_context(|| format!("captain `{captain}` not found (or never set sail)"))
}

async fn grant_cosmetic(pool: &PgPool, captain: &str, cosmetic: &str, by: &str) -> Result<()> {
    known_cosmetic(cosmetic)?;
    let character = captain_character(pool, captain).await?;
    let inserted = sqlx::query(
        "INSERT INTO character_cosmetics (character_id, cosmetic_id, granted_by) \
         VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(character)
    .bind(cosmetic)
    .bind(by)
    .execute(pool)
    .await
    .context("failed to grant the cosmetic")?;
    if inserted.rows_affected() == 0 {
        println!("{captain} already owns {cosmetic}");
    } else {
        println!("granted {cosmetic} to {captain} (shows up at the next login)");
    }
    Ok(())
}

async fn revoke_cosmetic(pool: &PgPool, captain: &str, cosmetic: &str) -> Result<()> {
    known_cosmetic(cosmetic)?;
    let character = captain_character(pool, captain).await?;
    let mut tx = pool.begin().await.context("failed to open a transaction")?;
    let removed =
        sqlx::query("DELETE FROM character_cosmetics WHERE character_id = $1 AND cosmetic_id = $2")
            .bind(character)
            .bind(cosmetic)
            .execute(&mut *tx)
            .await
            .context("failed to revoke the cosmetic")?;
    sqlx::query(
        "UPDATE characters SET \
         sail_cosmetic = NULLIF(sail_cosmetic, $2), flag_cosmetic = NULLIF(flag_cosmetic, $2) \
         WHERE id = $1",
    )
    .bind(character)
    .bind(cosmetic)
    .execute(&mut *tx)
    .await
    .context("failed to clear the worn cosmetic")?;
    tx.commit().await.context("failed to commit the revoke")?;
    if removed.rows_affected() == 0 {
        println!("{captain} did not own {cosmetic}");
    } else {
        println!("revoked {cosmetic} from {captain}");
    }
    Ok(())
}

async fn list_cosmetics(pool: &PgPool, captain: &str) -> Result<()> {
    let character = captain_character(pool, captain).await?;
    let owned: Vec<(
        String,
        String,
        sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    )> = sqlx::query_as(
        "SELECT cosmetic_id, granted_by, granted_at FROM character_cosmetics \
         WHERE character_id = $1 ORDER BY granted_at",
    )
    .bind(character)
    .fetch_all(pool)
    .await
    .context("failed to list the captain's cosmetics")?;
    for (id, by, at) in owned {
        println!("{id:<16} granted by {by} at {at}");
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
