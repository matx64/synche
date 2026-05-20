use tokio::sync::{
    Mutex, broadcast,
    mpsc::{self, Receiver, Sender},
};

/// An mpsc channel whose receiver is shareable across tasks via a
/// `Mutex`.
///
/// Use this when multiple owners (typically behind an `Arc`) need to
/// pull from the same single-consumer queue and arbitrate access
/// themselves. The sender side stays a plain `mpsc::Sender` and can be
/// cloned freely.
pub struct MutexChannel<K> {
    pub tx: Sender<K>,
    pub rx: Mutex<Receiver<K>>,
}

impl<K> MutexChannel<K> {
    pub fn new(buffer: usize) -> Self {
        let (tx, rx) = mpsc::channel(buffer);
        Self {
            tx,
            rx: Mutex::new(rx),
        }
    }

    /// Acquires the receiver lock and waits for the next message.
    ///
    /// Returns `None` once all senders have been dropped.
    pub async fn recv(&self) -> Option<K> {
        self.rx.lock().await.recv().await
    }
}

/// A fan-out broadcast channel whose subscribers are created on demand.
///
/// Used to push live updates (`ServerEvent`s) to every connected SSE
/// client. Late subscribers do not see messages produced before they
/// subscribed; slow subscribers may lag and lose messages per
/// `tokio::sync::broadcast` semantics.
pub struct BroadcastChannel<T: Clone> {
    tx: broadcast::Sender<T>,
}

impl<T: Clone> BroadcastChannel<T> {
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn sender(&self) -> broadcast::Sender<T> {
        self.tx.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.tx.subscribe()
    }
}
