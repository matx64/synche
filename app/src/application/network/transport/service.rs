use crate::{
    application::{
        EntryManager, PeerManager,
        network::transport::{
            interface::TransportInterface, receiver::TransportReceiver, sender::TransportSender,
        },
        persistence::interface::PersistenceInterface,
    },
    domain::{AppState, Channel, TransportChannelData},
};
use std::sync::Arc;
use tokio::{io, sync::mpsc::Sender};

pub struct TransportService<T: TransportInterface, P: PersistenceInterface> {
    sender: TransportSender<T, P>,
    receiver: TransportReceiver<T, P>,
}

impl<T: TransportInterface, P: PersistenceInterface> TransportService<T, P> {
    pub fn new(
        adapter: T,
        state: Arc<AppState>,
        peer_manager: Arc<PeerManager>,
        entry_manager: Arc<EntryManager<P>>,
    ) -> (Self, Sender<TransportChannelData>) {
        let adapter = Arc::new(adapter);
        let sender_chan = Channel::new(100);

        (
            Self {
                sender: TransportSender::new(
                    adapter.clone(),
                    state.clone(),
                    peer_manager.clone(),
                    entry_manager.clone(),
                    sender_chan.rx,
                ),
                receiver: TransportReceiver::new(
                    adapter,
                    state,
                    peer_manager,
                    entry_manager,
                    sender_chan.tx.clone(),
                ),
            },
            sender_chan.tx,
        )
    }

    pub async fn run(&self) -> io::Result<()> {
        tokio::select!(
            res = self.sender.run() => res,
            res = self.receiver.run() => res
        )
    }
}
