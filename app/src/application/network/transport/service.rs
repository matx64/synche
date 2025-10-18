use crate::{
    application::{
        AppState, EntryManager, PeerManager,
        network::transport::{
            interface::TransportInterfaceV2, receiverv2::TransportReceiverV2,
            senderv2::TransportSenderV2,
        },
        persistence::interface::PersistenceInterface,
    },
    domain::transport::TransportChannel,
};
use std::sync::Arc;
use tokio::io;

pub struct TransportService<T: TransportInterfaceV2, P: PersistenceInterface> {
    sender: TransportSenderV2<T, P>,
    receiver: TransportReceiverV2<T, P>,
}

impl<T: TransportInterfaceV2, P: PersistenceInterface> TransportService<T, P> {
    pub fn new(
        adapter: T,
        state: Arc<AppState>,
        peer_manager: Arc<PeerManager>,
        entry_manager: Arc<EntryManager<P>>,
    ) -> Self {
        let adapter = Arc::new(adapter);
        let sender_chan = TransportChannel::new();

        Self {
            sender: TransportSenderV2::new(
                adapter.clone(),
                state.clone(),
                peer_manager.clone(),
                entry_manager.clone(),
                sender_chan.rx,
            ),
            receiver: TransportReceiverV2::new(
                adapter,
                state,
                peer_manager,
                entry_manager,
                sender_chan.tx,
            ),
        }
    }

    pub async fn run(&self) -> io::Result<()> {
        tokio::try_join!(self.sender.run(), self.receiver.run())?;
        Ok(())
    }
}
