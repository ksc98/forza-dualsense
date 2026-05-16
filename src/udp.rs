use std::net::SocketAddr;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::net::UdpSocket;

use crate::state::SharedState;
use crate::telemetry::Telemetry;

/// Bind on `host:port` and forward the latest decoded telemetry packet
/// into shared state. Drains the kernel queue every iteration so we only
/// react to the freshest frame.
pub async fn run(state: SharedState, host: String, port: u16) -> Result<()> {
    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    let socket = UdpSocket::bind(addr)
        .await
        .with_context(|| format!("failed to bind UDP {addr} (already in use? firewall?)"))?;
    tracing::info!("UDP listening on {addr}");

    let mut buf = [0u8; 2048];

    loop {
        // Block until something arrives.
        let n = match socket.recv(&mut buf).await {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!("UDP recv error: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };

        // Drain anything else already queued so we use only the latest.
        let mut latest_len = n;
        while let Ok(more) = socket.try_recv(&mut buf) {
            latest_len = more;
        }

        if let Some(tel) = Telemetry::parse(&buf[..latest_len]) {
            let mut s = state.lock();
            s.telemetry = tel;
            s.packets_received = s.packets_received.saturating_add(1);
            s.last_packet_at = Some(Instant::now());
        }
    }
}
