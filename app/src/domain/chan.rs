use tokio::sync::{
    Mutex, broadcast,
    mpsc::{self, Receiver, Sender},
};

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

    pub async fn recv(&self) -> Option<K> {
        self.rx.lock().await.recv().await
    }
}

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
