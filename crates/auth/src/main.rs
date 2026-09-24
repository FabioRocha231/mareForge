//! Binário `marvyr-auth`. Configuração só por ambiente; qualquer item
//! obrigatório ausente ou inválido derruba a partida (fail closed).

use std::net::SocketAddr;
use std::process::ExitCode;

use marvyr_auth::{app, connect, redact_database_url, AppState};
use tracing_subscriber::EnvFilter;

const DEFAULT_ADDR: &str = "0.0.0.0:8080";
const DEFAULT_TTL_SECS: u64 = 7 * 24 * 60 * 60;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("marvyr-auth abortou: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let database_url =
        std::env::var("MARVYR_DATABASE_URL").map_err(|_| "MARVYR_DATABASE_URL não definida")?;
    let secret =
        std::env::var("MARVYR_JWT_SECRET").map_err(|_| "MARVYR_JWT_SECRET não definido")?;
    if secret.len() < marvyr_auth_token::MIN_SECRET_LEN {
        return Err(marvyr_auth_token::TokenError::WeakSecret.into());
    }
    let addr: SocketAddr = std::env::var("MARVYR_AUTH_ADDR")
        .unwrap_or_else(|_| DEFAULT_ADDR.to_owned())
        .parse()
        .map_err(|e| format!("MARVYR_AUTH_ADDR inválido: {e}"))?;
    let ttl_secs = match std::env::var("MARVYR_TOKEN_TTL_SECS") {
        Ok(raw) => raw
            .parse::<u64>()
            .ok()
            .filter(|ttl| *ttl > 0)
            .ok_or("MARVYR_TOKEN_TTL_SECS deve ser inteiro positivo")?,
        Err(_) => DEFAULT_TTL_SECS,
    };
    let trust_proxy = std::env::var("MARVYR_TRUST_PROXY").is_ok_and(|v| v == "1");

    tracing::info!("conectando ao banco {}", redact_database_url(&database_url));
    let pool = connect(&database_url)
        .await
        .map_err(|e| format!("banco indisponível ou migration falhou: {e}"))?;
    let state = AppState::new(pool, secret.into_bytes(), ttl_secs, trust_proxy)?;

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, ttl_secs, trust_proxy, "marvyr-auth ouvindo");
    axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    tracing::info!("marvyr-auth encerrado");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::warn!("sem handler de Ctrl+C: {e}");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(e) => {
                tracing::warn!("sem handler de SIGTERM: {e}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("sinal recebido, encerrando");
}
