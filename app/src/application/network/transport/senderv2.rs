use crate::{
    application::network::transport::interface::TransportInterfaceV2,
    domain::{
        EntryInfo,
        transport::{HandshakeKind, TransportChannel, TransportSendData},
    },
};
use std::net::IpAddr;
use tokio::io;

pub struct TransportSenderV2<T: TransportInterfaceV2> {
    adapter: T,
    send_chan: TransportChannel<TransportSendData>,
    control_chan: TransportChannel<TransportSendData>,
    transfer_chan: TransportChannel<(IpAddr, EntryInfo)>,
}

impl<T: TransportInterfaceV2> TransportSenderV2<T> {
    pub async fn run(&self) -> io::Result<()> {
        tokio::try_join!(self.send(), self.send_control(), self.send_files())?;
        Ok(())
    }

    async fn send(&self) -> io::Result<()> {
        while let Some(data) = self.send_chan.rx.lock().await.recv().await {
            match data {
                TransportSendData::Transfer(data) => {
                    self.transfer_chan
                        .tx
                        .send(data)
                        .await
                        .map_err(io::Error::other)?;
                }

                _ => {
                    self.control_chan
                        .tx
                        .send(data)
                        .await
                        .map_err(io::Error::other)?;
                }
            }
        }
        Ok(())
    }

    async fn send_control(&self) -> io::Result<()> {
        while let Some(data) = self.control_chan.rx.lock().await.recv().await {
            match data {
                TransportSendData::Handshake((target, kind)) => {
                    self.send_handshake(target, kind).await?;
                }

                TransportSendData::Metadata(entry) => {
                    self.send_metadata(entry).await?;
                }

                TransportSendData::Request((target, entry)) => {
                    self.send_request(target, entry).await?;
                }

                _ => unreachable!(),
            }
        }
        Ok(())
    }

    async fn send_handshake(&self, target: IpAddr, kind: HandshakeKind) -> io::Result<()> {
        Ok(())
    }

    async fn send_metadata(&self, entry: EntryInfo) -> io::Result<()> {
        Ok(())
    }

    async fn send_request(&self, target: IpAddr, entry: EntryInfo) -> io::Result<()> {
        Ok(())
    }

    async fn send_files(&self) -> io::Result<()> {
        while let Some((target, entry)) = self.transfer_chan.rx.lock().await.recv().await {}
        Ok(())
    }
}
