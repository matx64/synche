use crate::{
    application::{
        AppState,
        network::transport::interface::{
            HandshakeData, HandshakeKind, TransportDataV2, TransportInterfaceV2,
        },
    },
    domain::EntryInfo,
};
use std::{net::IpAddr, sync::Arc};
use tokio::{
    io,
    sync::{
        Mutex,
        mpsc::{self, Receiver, Sender},
    },
};

pub struct TransportService<T: TransportInterfaceV2> {
    adapter: T,
    state: Arc<AppState>,
    send_rx: Mutex<Receiver<(IpAddr, TransportDataV2)>>,
    data_chan: ReceiverChannel<EntryInfo>,
    control_chan: ReceiverChannel<TransportDataV2>,
}

impl<T: TransportInterfaceV2> TransportService<T> {
    pub fn new(adapter: T, state: Arc<AppState>) -> (Self, Sender<(IpAddr, TransportDataV2)>) {
        let (send_tx, send_rx) = mpsc::channel(100);

        (
            Self {
                state,
                adapter,
                send_rx: Mutex::new(send_rx),
                data_chan: ReceiverChannel::new(),
                control_chan: ReceiverChannel::new(),
            },
            send_tx,
        )
    }

    pub async fn run(&self) -> io::Result<()> {
        tokio::try_join!(self.send(), self.recv(), self.recv_data())?;
        Ok(())
    }

    async fn send(&self) -> io::Result<()> {
        while let Some((target, data)) = self.send_rx.lock().await.recv().await {
            let _ = self.adapter.send(target, data).await;
        }
        Ok(())
    }

    async fn recv(&self) -> io::Result<()> {
        loop {
            match self.adapter.recv().await? {
                TransportDataV2::Transfer(entry) => {
                    let _ = self.data_chan.tx.send(entry).await;
                }

                other => {
                    let _ = self.control_chan.tx.send(other).await;
                }
            }
        }
    }

    async fn recv_data(&self) -> io::Result<()> {
        while let Some(entry) = self.data_chan.rx.lock().await.recv().await {
            self.handle_transfer(entry).await?;
        }
        Ok(())
    }

    async fn recv_control(&self) -> io::Result<()> {
        while let Some(data) = self.control_chan.rx.lock().await.recv().await {
            match data {
                TransportDataV2::Handshake((data, kind)) => todo!(),
                TransportDataV2::Metadata(entry) => todo!(),
                TransportDataV2::Request(entry) => todo!(),

                _ => unreachable!(),
            }
        }
        Ok(())
    }

    async fn handle_transfer(&self, entry: EntryInfo) -> io::Result<()> {
        Ok(())
    }

    async fn handle_handshake(&self, data: HandshakeData, kind: HandshakeKind) -> io::Result<()> {
        Ok(())
    }

    async fn handle_metadata(&self, entry: EntryInfo) -> io::Result<()> {
        Ok(())
    }
}

pub struct ReceiverChannel<K> {
    tx: Sender<K>,
    rx: Mutex<Receiver<K>>,
}

impl<K> ReceiverChannel<K> {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(16);
        Self {
            tx,
            rx: Mutex::new(rx),
        }
    }
}
