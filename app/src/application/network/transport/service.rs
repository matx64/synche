use crate::{
    application::{
        AppState,
        network::transport::interface::{TransportDataV2, TransportInterfaceV2},
    },
    domain::EntryInfo,
};
use std::sync::Arc;
use tokio::{
    io,
    sync::mpsc::{self, Receiver, Sender},
};

pub struct TransportService<T: TransportInterfaceV2> {
    adapter: T,
    state: Arc<AppState>,
    data_chan: (Sender<EntryInfo>, Receiver<EntryInfo>),
    control_chan: (Sender<TransportDataV2>, Receiver<TransportDataV2>),
}

impl<T: TransportInterfaceV2> TransportService<T> {
    pub fn new(adapter: T, state: Arc<AppState>) -> Self {
        let data_chan = mpsc::channel(16);
        let control_chan = mpsc::channel(16);

        Self {
            adapter,
            state,
            data_chan,
            control_chan,
        }
    }

    pub async fn send(&self) -> io::Result<()> {
        Ok(())
    }

    pub async fn recv(&self) -> io::Result<()> {
        loop {
            match self.adapter.recv().await? {
                TransportDataV2::Transfer(info) => {
                    let _ = self.data_chan.0.send(info).await;
                }

                other => {
                    let _ = self.control_chan.0.send(other).await;
                }
            }
        }
    }

    pub async fn handle_transfer(&self) {}
}
