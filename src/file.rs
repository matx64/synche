use crate::models::file::SynchedFile;
use sha2::{Digest, Sha256};
use std::{
    net::SocketAddr,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    fs::File,
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tracing::info;

const TCP_PORT: u16 = 8889;

pub struct ReceivedFile {
    pub name: String,
    pub size: u64,
    pub contents: Vec<u8>,
    pub hash: String,
    pub last_modified_at: SystemTime,
}

pub async fn send_file(
    synched_file: &SynchedFile,
    mut target_addr: SocketAddr,
) -> std::io::Result<()> {
    let path = Path::new("synche-files").join(&synched_file.name);
    let file_name = path.file_name().unwrap().to_string_lossy().into_owned();

    info!("Sending file {} to {}", file_name, target_addr);

    // Read file content
    let mut file = File::open(path).await?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).await?;

    // Get last modified at
    let last_modified_at = synched_file
        .last_modified_at
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Connect to target device's TCP server
    target_addr.set_port(TCP_PORT);
    let mut stream = TcpStream::connect(target_addr).await?;

    // Send file name length (u64)
    stream
        .write_all(&u64::to_be_bytes(file_name.len() as u64))
        .await?;

    // Send file name
    stream.write_all(file_name.as_bytes()).await?;

    // Send file hash (32 bytes)
    let hash_bytes = hex::decode(&synched_file.hash).unwrap();
    stream.write_all(&hash_bytes).await?;

    // Send file size (u64)
    let file_size = buffer.len() as u64;
    stream.write_all(&u64::to_be_bytes(file_size)).await?;

    // Send file contents
    stream.write_all(&buffer).await?;

    // Send last modification date
    stream.write_all(&last_modified_at.to_be_bytes()).await?;

    info!(
        "Sent file: {} ({} bytes) to {}",
        file_name, file_size, target_addr
    );
    Ok(())
}

pub async fn read_file(socket: &mut TcpStream) -> std::io::Result<ReceivedFile> {
    // Read file name length (u64)
    let mut name_len_buf = [0u8; 8];
    socket.read_exact(&mut name_len_buf).await?;
    let name_len = u64::from_be_bytes(name_len_buf) as usize;

    // Read file name
    let mut file_name_buf = vec![0u8; name_len];
    socket.read_exact(&mut file_name_buf).await?;
    let file_name = String::from_utf8_lossy(&file_name_buf).into_owned();

    // Read file hash (32 bytes)
    let mut hash_buf = [0u8; 32];
    socket.read_exact(&mut hash_buf).await?;
    let received_hash = hex::encode(hash_buf);

    // Read file size (u64)
    let mut file_size_buf = [0u8; 8];
    socket.read_exact(&mut file_size_buf).await?;
    let file_size = u64::from_be_bytes(file_size_buf);

    // Read file contents
    let mut file_buf = vec![0u8; file_size as usize];
    socket.read_exact(&mut file_buf).await?;

    // Compute hash for corruption check
    let computed_hash = format!("{:x}", Sha256::digest(&file_buf));
    if computed_hash != received_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Hash mismatch: data corruption detected",
        ));
    }

    // Read file last modification date
    let mut timestamp_buf = [0u8; 8];
    socket.read_exact(&mut timestamp_buf).await?;

    let last_modified_at = UNIX_EPOCH + Duration::from_secs(u64::from_be_bytes(timestamp_buf));

    Ok(ReceivedFile {
        name: file_name,
        size: file_size,
        contents: file_buf,
        hash: received_hash,
        last_modified_at,
    })
}
