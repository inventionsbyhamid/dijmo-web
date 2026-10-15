//! `dijmo-web`: serves the site. `dijmo-web --healthcheck` probes a running
//! server's `/healthz` (for Docker's HEALTHCHECK; the runtime image has no curl).

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use dijmo_core::Config;
use dijmo_core::config::EmailProvider;
use dijmo_web::assets::Assets;
use dijmo_web::email::{EmailSender, LogSender, ResendSender};
use dijmo_web::state::AppState;
use dijmo_web::worker::Worker;
use tokio::sync::watch;
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    if std::env::args().any(|a| a == "--healthcheck") {
        return healthcheck();
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    match runtime.block_on(serve()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "dijmo-web stopped with an error");
            eprintln!("dijmo-web: {e}");
            ExitCode::FAILURE
        }
    }
}

/// GET /healthz on 127.0.0.1:$PORT; exit 0 on a 200.
fn healthcheck() -> ExitCode {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.trim().parse().ok())
        .unwrap_or(8080);
    let probe = || -> std::io::Result<bool> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
        stream.set_write_timeout(Some(Duration::from_secs(3)))?;
        stream
            .write_all(b"GET /healthz HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
        let mut head = [0u8; 16];
        let n = stream.read(&mut head)?;
        let line = String::from_utf8_lossy(&head[..n]);
        Ok(line.starts_with("HTTP/1.1 200") || line.starts_with("HTTP/1.0 200"))
    };
    match probe() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => {
            eprintln!("healthcheck: /healthz did not answer 200");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("healthcheck: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `LOG_FORMAT=json` for structured logs in production; text otherwise.
/// Read before the rest of the config so config errors are logged too.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=info"));
    let json = std::env::var("LOG_FORMAT").is_ok_and(|f| f.trim().eq_ignore_ascii_case("json"));
    if json {
        tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_span_list(false)
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

async fn serve() -> Result<(), BoxError> {
    init_tracing();
    let config = Config::from_env()?;

    if matches!(config.email_provider, EmailProvider::Log) && config.public_base_url.is_https() {
        tracing::warn!(
            "EMAIL_PROVIDER is `log` on an https site: confirmation emails are only logged, not sent. Set RESEND_API_KEY and EMAIL_FROM."
        );
    }

    let pool = connect_with_retry(&config.database_url).await?;
    dijmo_core::migrate(&pool).await?;
    tracing::info!("migrations applied");

    let assets = Assets::load(&config.assets_dir)?;
    tracing::info!(css = %assets.css.url, js = %assets.js.url, "assets hashed");

    let sender: Arc<dyn EmailSender> = match &config.email_provider {
        EmailProvider::Log => Arc::new(LogSender),
        EmailProvider::Resend { api_key } => Arc::new(ResendSender::new(
            api_key.clone(),
            config.email_from.clone(),
        )),
    };
    tracing::info!(provider = ?config.email_provider, "email sender ready");

    let state = AppState::from_config(&config, pool.clone(), assets);
    let worker = Worker {
        pool: pool.clone(),
        sender,
        base_url: state.base_url.clone(),
        keys: state.keys.clone(),
        policy: state.policy,
        wake: state.outbox_wake.clone(),
    };
    let (stop_tx, stop_rx) = watch::channel(false);
    let worker_task = tokio::spawn(worker.run(stop_rx));

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, base_url = config.public_base_url.as_str(), "listening");

    // Stop taking requests on SIGTERM/Ctrl-C, let in-flight ones finish, but
    // never hang for more than 20 s.
    let (signal_tx, mut signal_rx) = watch::channel(false);
    let server = axum::serve(
        listener,
        dijmo_web::app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown_signal().await;
        tracing::info!("shutting down");
        let _ = signal_tx.send(true);
    });
    let deadline = async move {
        let _ = signal_rx.wait_for(|s| *s).await;
        tokio::time::sleep(Duration::from_secs(20)).await;
    };
    tokio::select! {
        r = server => r?,
        _ = deadline => tracing::warn!("in-flight requests did not finish in 20 s; stopping anyway"),
    }

    let _ = stop_tx.send(true);
    if tokio::time::timeout(Duration::from_secs(10), worker_task)
        .await
        .is_err()
    {
        tracing::warn!("outbox worker did not stop in 10 s");
    }
    pool.close().await;
    tracing::info!("stopped");
    Ok(())
}

/// The database may start after us (e.g. on a fresh deploy): retry for ~1 min.
async fn connect_with_retry(url: &str) -> Result<sqlx::PgPool, BoxError> {
    let mut wait = Duration::from_millis(500);
    let mut attempt = 1u32;
    loop {
        match dijmo_core::connect(url).await {
            Ok(pool) => return Ok(pool),
            Err(e) if attempt < 10 => {
                tracing::warn!(attempt, error = %e, "database not reachable yet; retrying");
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(Duration::from_secs(10));
                attempt += 1;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
}
