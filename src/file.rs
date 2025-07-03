use crate::{Device, config::SynchedFile};
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::Path,
    sync::{Arc, RwLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
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
    devices: Arc<RwLock<HashMap<SocketAddr, Device>>>,
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
    devices: Arc<RwLock<HashMap<SocketAddr, Device>>>,
) -> std::io::Result<()> {
    let src_addr = socket.peer_addr()?;

    println!("Handling received file from: {}", src_addr);

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

    // Read file last modification date
    let mut timestamp_buf = [0u8; 8];
    socket.read_exact(&mut timestamp_buf).await?;
    let timestamp = u64::from_be_bytes(timestamp_buf);

    let last_modified_date = UNIX_EPOCH + Duration::from_secs(timestamp);

    // Save file
    let mut file = File::create(&format!("synche-files/{}", file_name)).await?;
    file.write_all(&file_buf).await?;

    if let Ok(mut devices) = devices.write() {
        if let Some(device) = devices.get_mut(&src_addr) {
            if let Some(file) = device.synched_files.get_mut(&file_name) {
                file.last_modified_at = last_modified_date;
            }
        }
    }

    if let Ok(mut files) = synched_files.write() {
        if let Some(file) = files.get_mut(&file_name) {
            file.last_modified_at = last_modified_date;
        }
    }

    println!(
        "Received file: {} ({} bytes) from {}",
        file_name, file_size, src_addr
    );
    Ok(())
}

pub async fn send_file(path: &str, mut target_addr: SocketAddr) -> std::io::Result<()> {
    let path = Path::new(path);
    let file_name = path.file_name().unwrap().to_string_lossy().into_owned();

    println!("Sending file {} to {}", file_name, target_addr);

    // Read file content
    let mut file = File::open(path).await?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).await?;

    // Get metadata & mod time
    let metadata = file.metadata().await?;
    let modified = metadata.modified().unwrap_or(SystemTime::now());
    let timestamp = modified.duration_since(UNIX_EPOCH).unwrap().as_secs();

    // Connect to target device's TCP server
    target_addr.set_port(TCP_PORT);
    let mut stream = TcpStream::connect(target_addr).await?;

    // Send file name length (u64)
    stream
        .write_all(&u64::to_be_bytes(file_name.len() as u64))
        .await?;

    // Send file name
    stream.write_all(file_name.as_bytes()).await?;

    // Send file size (u64)
    let file_size = buffer.len() as u64;
    stream.write_all(&u64::to_be_bytes(file_size)).await?;

    // Send file contents
    stream.write_all(&buffer).await?;

    // Send last modification date
    stream.write_all(&timestamp.to_be_bytes()).await?;

    println!(
        "Sent file: {} ({} bytes) to {}",
        file_name, file_size, target_addr
    );
    Ok(())
}

pub async fn sync_files(
    mut sync_rx: Receiver<SynchedFile>,
    devices: Arc<RwLock<HashMap<SocketAddr, Device>>>,
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
                            if let Err(err) = send_file(&format!("synche-files/{}", &file.name), device.addr).await {
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
