use tokio::sync::{
    Mutex,
    mpsc::{self, Receiver, Sender},
};

pub struct Channel<K> {
    pub tx: Sender<K>,
    pub rx: Mutex<Receiver<K>>,
}

impl<K> Channel<K> {
    pub fn new(buffer: usize) -> Self {
        let (tx, rx) = mpsc::channel(buffer);
        Self {
            tx,
            rx: Mutex::new(rx),
        }
    }
}
