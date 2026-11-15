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
    // TODO: resolve when the first of these resolves:
    //   tokio::signal::ctrl_c()
    //   #[cfg(unix)] tokio::signal::unix::signal(SignalKind::terminate()), then `.recv()`
    //   #[cfg(not(unix))] std::future::pending::<()>()
    // If a handler fails to install, log it and let that branch pend forever. Resolving
    // instead would stop the server the moment it starts.
    todo!("wait for Ctrl-C or SIGTERM")
}
