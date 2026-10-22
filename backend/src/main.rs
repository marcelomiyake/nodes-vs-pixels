use std::env;

use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};

const DEFAULT_ADDR: &str = "0.0.0.0:50051";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = env::var("BACKEND_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_owned());
    let listener = TcpListener::bind(&addr).await?;
    let mut terminate = signal(SignalKind::terminate())?;
    println!("signup backend listening on {}", listener.local_addr()?);
    backend::serve(listener, async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    })
    .await?;
    Ok(())
}
