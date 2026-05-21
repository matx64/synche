//! Shared test helpers for the transport module.
//!
//! `RecordingTransport` is an in-memory `TransportInterface` that lets
//! tests seed inbound events, capture outbound sends, and toggle a
//! permanent-failure mode for retry tests.

use std::{
    net::IpAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::{Mutex, mpsc};

use crate::{
    application::network::transport::interface::{
        TransportError, TransportInterface, TransportResult,
    },
    domain::{TransportData, TransportEvent},
};

pub(super) struct RecordingTransport {
    pub sends: Arc<Mutex<Vec<(IpAddr, TransportData)>>>,
    pub recv_tx: mpsc::UnboundedSender<TransportResult<TransportEvent>>,
    pub recv_rx: Mutex<mpsc::UnboundedReceiver<TransportResult<TransportEvent>>>,
    pub fail_sends: Arc<AtomicBool>,
}

impl RecordingTransport {
    pub fn new() -> Self {
        let (recv_tx, recv_rx) = mpsc::unbounded_channel();
        Self {
            sends: Arc::new(Mutex::new(Vec::new())),
            recv_tx,
            recv_rx: Mutex::new(recv_rx),
            fail_sends: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns a cheap handle that lets tests push events into `recv`
    /// after the transport has been moved into a service.
    pub fn push_handle(&self) -> mpsc::UnboundedSender<TransportResult<TransportEvent>> {
        self.recv_tx.clone()
    }

    pub async fn sends_count(&self) -> usize {
        self.sends.lock().await.len()
    }

    pub fn set_fail_sends(&self, fail: bool) {
        self.fail_sends.store(fail, Ordering::SeqCst);
    }
}

impl TransportInterface for RecordingTransport {
    async fn recv(&self) -> TransportResult<TransportEvent> {
        self.recv_rx
            .lock()
            .await
            .recv()
            .await
            .unwrap_or_else(|| Err(TransportError::new("recv channel closed")))
    }

    async fn send(&self, target: IpAddr, data: TransportData) -> TransportResult<()> {
        if self.fail_sends.load(Ordering::SeqCst) {
            return Err(TransportError::new("simulated send failure"));
        }
        self.sends.lock().await.push((target, data));
        Ok(())
    }
}
