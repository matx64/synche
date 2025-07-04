use crate::{
    Device,
    config::SynchedFile,
    file::{read_file, send_file},
};
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::{
    fs::File,
    io::{self, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc::Receiver,
    time,
};

const TCP_PORT: u16 = 8889;

pub async fn recv_files(
    synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
    devices: Arc<RwLock<HashMap<IpAddr, Device>>>,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", TCP_PORT)).await?;
    loop {
        let (mut socket, _addr) = listener.accept().await?;

        let files = synched_files.clone();
        let devices = devices.clone();

        tokio::spawn(async move { handle_file(&mut socket, files, devices).await });
    }
}

async fn handle_file(
    socket: &mut TcpStream,
    synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
    devices: Arc<RwLock<HashMap<IpAddr, Device>>>,
) -> std::io::Result<()> {
    let src_addr = socket.peer_addr()?;

    println!("Handling received file from: {}", src_addr);

    let recv_file = read_file(socket).await?;

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

    println!(
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
                println!("File added to buffer: {}", file.name);
                buffer.insert(file.name.clone(), file);
            }

            _ = interval.tick() => {
                if buffer.is_empty() {
                    continue;
                }

                println!("Synching files: {:?}", buffer);

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
                                eprintln!("Error synching file `{}`: {}", &file.name, err);
                            }
                        }
                    }
                }

                buffer.clear();
            }
        }
    }
}
