use crate::application::{
    AppState,
    network::transport::interface::{
        TransportDataV2, TransportInterfaceV2, TransportRecvData, TransportSendData,
    },
};
use std::sync::Arc;
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
    send_rx: Mutex<Receiver<TransportSendData>>,
    data_chan: ReceiverChannel<TransportRecvData>,
    control_chan: ReceiverChannel<TransportRecvData>,
}

impl<T: TransportInterfaceV2> TransportService<T> {
    pub fn new(adapter: T, state: Arc<AppState>) -> (Self, Sender<TransportSendData>) {
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
        tokio::try_join!(
            self.send(),
            self.recv(),
            self.recv_data(),
            self.recv_control()
        )?;
        Ok(())
    }

    async fn send(&self) -> io::Result<()> {
        while let Some(data) = self.send_rx.lock().await.recv().await {
            let _ = self.adapter.send(data).await;
        }
        Ok(())
    }

    async fn recv(&self) -> io::Result<()> {
        loop {
            let data = self.adapter.recv().await?;
            match data.data {
                TransportDataV2::Transfer(_) => {
                    let _ = self.data_chan.tx.send(data).await;
                }

                _ => {
                    let _ = self.control_chan.tx.send(data).await;
                }
            }
        }
    }

    async fn recv_data(&self) -> io::Result<()> {
        while let Some(data) = self.data_chan.rx.lock().await.recv().await {
            self.handle_transfer(data).await?;
        }
        Ok(())
    }

    async fn recv_control(&self) -> io::Result<()> {
        while let Some(data) = self.control_chan.rx.lock().await.recv().await {
            match data.data {
                TransportDataV2::Handshake(_) => {
                    self.handle_handshake(data).await?;
                }

                TransportDataV2::Metadata(_) => {
                    self.handle_metadata(data).await?;
                }

                TransportDataV2::Request(_) => {
                    self.handle_request(data).await?;
                }

                _ => unreachable!(),
            }
        }
        Ok(())
    }

    async fn handle_handshake(&self, data: TransportRecvData) -> io::Result<()> {
        Ok(())
    }

    async fn handle_metadata(&self, data: TransportRecvData) -> io::Result<()> {
        Ok(())
    }

    async fn handle_request(&self, data: TransportRecvData) -> io::Result<()> {
        Ok(())
    }

    async fn handle_transfer(&self, data: TransportRecvData) -> io::Result<()> {
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
