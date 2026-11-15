use std::{io, net::Ipv4Addr};

use axum::Router;
use tokio::net::TcpListener;

/// Serves `router` on `port` until Ctrl-C, or SIGTERM on Unix. Then it stops accepting
/// connections and returns once every open connection has finished, which drops `router`.
pub async fn serve(port: u16, router: Router) -> io::Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
}

async fn shutdown_signal() {
    let interrupt = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "could not install Ctrl-C handler");
            std::future::pending::<()>().await;
        }
    };
    let terminate = async {
        #[cfg(unix)]
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "could not install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
        #[cfg(not(unix))]
        std::future::pending::<()>().await;
    };
    tokio::select! {
        () = interrupt => {},
        () = terminate => {},
    }
}
