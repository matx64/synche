use crate::{
    Device,
    config::SynchedFile,
    file::{read_file, send_file},
    handshake::HandshakeHandler,
};
use std::{
    collections::HashMap,
    io::ErrorKind,
    net::IpAddr,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::{
    fs::File,
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc::Receiver,
    time,
};
use tracing::{error, info};

const TCP_PORT: u16 = 8889;

#[repr(u8)]
pub enum SyncDataKind {
    HandshakeRequest = 0,
    HandshakeResponse = 1,
    File = 2,
}

pub async fn recv_data(
    handshake_handler: Arc<HandshakeHandler>,
    synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
    devices: Arc<RwLock<HashMap<IpAddr, Device>>>,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", TCP_PORT)).await?;
    loop {
        let (mut stream, _addr) = listener.accept().await?;

        let mut kind_buf = [0u8; 1];
        stream.read_exact(&mut kind_buf).await?;
        let kind = SyncDataKind::try_from(kind_buf[0])?;

        match kind {
            SyncDataKind::File => {
                handle_file(&mut stream, synched_files.clone(), devices.clone()).await?;
            }
            SyncDataKind::HandshakeRequest => {
                handshake_handler.read_handshake(&mut stream, true).await?;
            }
            SyncDataKind::HandshakeResponse => {
                handshake_handler.read_handshake(&mut stream, false).await?;
            }
        };

        // let files = synched_files.clone();
        // let devices = devices.clone();

        // tokio::spawn(async move { handle_file(&mut stream, files, devices).await });
    }
}

async fn handle_file(
    stream: &mut TcpStream,
    synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
    devices: Arc<RwLock<HashMap<IpAddr, Device>>>,
) -> std::io::Result<()> {
    let src_addr = stream.peer_addr()?;

    info!("Handling received file from: {}", src_addr);

    let recv_file = read_file(stream).await?;

    if let Ok(mut devices) = devices.write() {
        if let Some(device) = devices.get_mut(&src_addr.ip()) {
            if let Some(file) = device.synched_files.get_mut(&recv_file.name) {
                file.last_modified_at = recv_file.last_modified_at;
            }
        }
    }

    if let Ok(mut files) = synched_files.write() {
        if let Some(local_file) = files.get_mut(&recv_file.name) {
            local_file.last_modified_at = recv_file.last_modified_at;
        }
    }

    // Save file
    let mut file = File::create(&format!("synche-files/{}", recv_file.name)).await?;
    file.write_all(&recv_file.contents).await?;

    info!(
        "Received file: {} ({} bytes) from {}",
        recv_file.name, recv_file.size, src_addr
    );
    Ok(())
}

pub async fn sync_files(
    mut sync_rx: Receiver<SynchedFile>,
    devices: Arc<RwLock<HashMap<IpAddr, Device>>>,
) -> io::Result<()> {
    let mut buffer = HashMap::<String, SynchedFile>::new();
    let mut interval = time::interval(Duration::from_secs(10));

    loop {
        tokio::select! {
            Some(file) = sync_rx.recv() => {
                info!("File added to buffer: {}", file.name);
                buffer.insert(file.name.clone(), file);
            }

            _ = interval.tick() => {
                if buffer.is_empty() {
                    continue;
                }

                info!("Synching files: {:?}", buffer);

                let devices = if let Ok(devices) = devices.read() {
                    devices
                        .values()
                        .filter(|device| {
                            buffer.values().any(|f| {
                                device.synched_files
                                    .get(&f.name)
                                    .map(|found| found.last_modified_at < f.last_modified_at)
                                    .unwrap_or(false)
                            })
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    continue;
                };

                for device in devices {
                    for file in buffer.values() {
                        if device.synched_files.get(&file.name).map(|f| f.last_modified_at < file.last_modified_at).unwrap_or(false) {
                            if let Err(err) = send_file(file, device.addr).await {
                                error!("Error synching file `{}`: {}", &file.name, err);
                            }
                        }
                    }
                }

                buffer.clear();
            }
        }
    }
}

impl TryFrom<u8> for SyncDataKind {
    type Error = io::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SyncDataKind::HandshakeRequest),
            1 => Ok(SyncDataKind::HandshakeResponse),
            2 => Ok(SyncDataKind::File),
            _ => Err(io::Error::new(
                ErrorKind::InvalidData,
                "Invalid SyncDataKind value",
            )),
        }
    }
}
