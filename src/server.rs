use std::{future::pending, io, net::Ipv4Addr};

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

/// Resolves on the first Ctrl-C, or SIGTERM on Unix. A listener that fails to install is
/// logged and never resolves, because resolving would stop the server the moment it starts.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!("cannot listen for Ctrl-C: {error}");
            pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                terminate.recv().await;
            }
            Err(error) => {
                tracing::error!("cannot listen for SIGTERM: {error}");
                pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}
