use crate::{Device, config::SynchedFile};
use chrono::Utc;
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::Path,
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

const TCP_PORT: u16 = 8889;

pub async fn recv_files(
    synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", TCP_PORT)).await?;
    loop {
        let (mut socket, _addr) = listener.accept().await?;
        let files = synched_files.clone();
        tokio::spawn(async move { handle_file(&mut socket, files).await });
    }
}

async fn handle_file(
    socket: &mut TcpStream,
    synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
) -> std::io::Result<()> {
    println!("Handling File...");

    // Read file name length (u64)
    let mut name_len_buf = [0u8; 8];
    socket.read_exact(&mut name_len_buf).await?;
    let name_len = u64::from_be_bytes(name_len_buf) as usize;

    // Read file name
    let mut file_name_buf = vec![0u8; name_len];
    socket.read_exact(&mut file_name_buf).await?;
    let file_name = String::from_utf8_lossy(&file_name_buf).into_owned();

    // Read file size (u64)
    let mut file_size_buf = [0u8; 8];
    socket.read_exact(&mut file_size_buf).await?;
    let file_size = u64::from_be_bytes(file_size_buf);

    // Read file contents
    let mut file_buf = vec![0u8; file_size as usize];
    socket.read_exact(&mut file_buf).await?;

    // Save file
    let mut file = File::create(&file_name).await?;
    file.write_all(&file_buf).await?;

    match synched_files.write() {
        Ok(mut files) => {
            if let Some(file) = files.get_mut(&file_name) {
                file.last_modified_at = Utc::now();
            }
        }
        Err(err) => {
            eprintln!("Failed to write files: {}", err);
        }
    }

    println!(
        "Received file: {} ({} bytes) from {}",
        file_name,
        file_size,
        socket.peer_addr()?
    );
    Ok(())
}

pub async fn send_file<P: AsRef<Path>>(
    file_path: P,
    mut target_addr: SocketAddr,
) -> std::io::Result<()> {
    println!("Sending File...");

    let file_path = file_path.as_ref();
    let mut file = File::open(file_path).await?;
    let file_size = file.metadata().await?.len();
    let file_name = file_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    target_addr.set_port(TCP_PORT);

    // Connect to target device's TCP server
    let mut socket = TcpStream::connect(target_addr).await?;

    // Send file name length (u64)
    socket
        .write_all(&u64::to_be_bytes(file_name.len() as u64))
        .await?;

    // Send file name
    socket.write_all(file_name.as_bytes()).await?;

    // Send file size (u64)
    socket.write_all(&u64::to_be_bytes(file_size)).await?;

    // Send file contents
    let mut file_buf = Vec::new();
    file.read_to_end(&mut file_buf).await?;
    socket.write_all(&file_buf).await?;

    println!(
        "Sent file: {} ({} bytes) to {}",
        file_name, file_size, target_addr
    );
    Ok(())
}

pub async fn sync_files(
    mut sync_rx: Receiver<String>,
    devices: Arc<RwLock<HashMap<SocketAddr, Device>>>,
) -> io::Result<()> {
    let mut buffer = HashSet::<String>::new();
    let mut interval = time::interval(Duration::from_secs(10));

    loop {
        tokio::select! {
            Some(file_name) = sync_rx.recv() => {
                println!("File name added to buffer: {}", file_name);
                buffer.insert(file_name);
            }

            _ = interval.tick() => {
                if buffer.is_empty() {
                    continue;
                }

                println!("Synching files: {:?}", buffer);

                let devices = {
                    let devices = devices.read().unwrap();
                    devices
                        .values()
                        .filter(|d| {
                            buffer
                                .iter()
                                .any(|file_name| d.synched_files.contains_key(file_name))
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                };

                for device in devices {
                    for file_name in &buffer {
                        if device.synched_files.contains_key(file_name) {
                            if let Err(err) = send_file(&format!("/synche-files/{}", file_name), device.addr).await {
                                eprintln!("Error synching file: {}", err);
                            }
                        }
                    }
                }

                buffer.clear();
            }
        }
    }
}
