use crate::{
    config::AppState,
    domain::{
        file::FileInfo,
        sync::{SyncFileKind, SyncKind},
    },
};
use sha2::{Digest, Sha256};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio::{
    fs::{self, File as FsFile},
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tracing::info;

pub struct ReceivedFile {
    pub name: String,
    pub version: u32,
    pub size: u64,
    pub contents: Vec<u8>,
    pub hash: String,
    pub from: IpAddr,
}

pub struct FileService {
    state: Arc<AppState>,
}

impl FileService {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub async fn send_metadata(
        &self,
        file: &FileInfo,
        mut target_addr: SocketAddr,
    ) -> std::io::Result<()> {
        let target_ip = target_addr.ip();

        info!("Sending {} metadata to {}", file.name, target_ip);

        // Connect to target peer's TCP server
        target_addr.set_port(self.state.constants.tcp_port);
        let mut stream = TcpStream::connect(target_addr).await?;

        // Send data kind
        stream
            .write_all(&[SyncKind::File(SyncFileKind::Metadata).as_u8()])
            .await?;

        // Send file name length (u64)
        stream
            .write_all(&u64::to_be_bytes(file.name.len() as u64))
            .await?;

        // Send file name
        stream.write_all(file.name.as_bytes()).await?;

        // Send file hash (32 bytes)
        let hash_bytes = hex::decode(&file.hash).unwrap();
        stream.write_all(&hash_bytes).await?;

        // Send local file version (u32)
        stream.write_all(&u32::to_be_bytes(file.version)).await?;

        info!("Successfully sent {} metadata to {}", file.name, target_ip);
        Ok(())
    }

    pub async fn read_metadata(
        &self,
        src_ip: IpAddr,
        stream: &mut TcpStream,
    ) -> std::io::Result<FileInfo> {
        let mut name_len_buf = [0u8; 8];
        stream.read_exact(&mut name_len_buf).await?;
        let name_len = u64::from_be_bytes(name_len_buf) as usize;

        let mut file_name_buf = vec![0u8; name_len];
        stream.read_exact(&mut file_name_buf).await?;
        let file_name = String::from_utf8_lossy(&file_name_buf).into_owned();

        let mut hash_buf = [0u8; 32];
        stream.read_exact(&mut hash_buf).await?;
        let received_hash = hex::encode(hash_buf);

        let mut version_buf = [0u8; 4];
        stream.read_exact(&mut version_buf).await?;
        let file_version = u32::from_be_bytes(version_buf);

        Ok(FileInfo {
            name: file_name,
            hash: received_hash,
            version: file_version,
            last_modified_by: Some(src_ip),
        })
    }

    pub async fn send_request(
        &self,
        file: &FileInfo,
        mut target_addr: SocketAddr,
    ) -> std::io::Result<()> {
        let target_ip = target_addr.ip();

        info!("Requesting file {} to {}", file.name, target_ip);

        // Connect to target peer's TCP server
        target_addr.set_port(self.state.constants.tcp_port);
        let mut stream = TcpStream::connect(target_addr).await?;

        // Send data kind
        stream
            .write_all(&[SyncKind::File(SyncFileKind::Request).as_u8()])
            .await?;

        // Send file name length (u64)
        stream
            .write_all(&u64::to_be_bytes(file.name.len() as u64))
            .await?;

        // Send file name
        stream.write_all(file.name.as_bytes()).await?;

        // Send file hash (32 bytes)
        let hash_bytes = hex::decode(&file.hash).unwrap();
        stream.write_all(&hash_bytes).await?;

        // Send local file version (u32)
        stream.write_all(&u32::to_be_bytes(file.version)).await?;

        info!("Successfully requested file {} to {}", file.name, target_ip);
        Ok(())
    }

    pub async fn read_request(&self, stream: &mut TcpStream) -> std::io::Result<FileInfo> {
        let mut name_len_buf = [0u8; 8];
        stream.read_exact(&mut name_len_buf).await?;
        let name_len = u64::from_be_bytes(name_len_buf) as usize;

        let mut file_name_buf = vec![0u8; name_len];
        stream.read_exact(&mut file_name_buf).await?;
        let file_name = String::from_utf8_lossy(&file_name_buf).into_owned();

        let mut hash_buf = [0u8; 32];
        stream.read_exact(&mut hash_buf).await?;
        let hash = hex::encode(hash_buf);

        let mut version_buf = [0u8; 4];
        stream.read_exact(&mut version_buf).await?;
        let file_version = u32::from_be_bytes(version_buf);

        Ok(FileInfo {
            name: file_name,
            hash,
            version: file_version,
            last_modified_by: None,
        })
    }

    pub async fn send_file(&self, file: &FileInfo, mut target_addr: SocketAddr) -> std::io::Result<()> {
        let target_ip = target_addr.ip();

        let path = self.state.constants.base_dir.join(&file.name);

        info!("Sending file {} to {}", file.name, target_ip);

        // Read file content
        let mut fs_file = FsFile::open(path).await?;
        let mut buffer = Vec::new();
        fs_file.read_to_end(&mut buffer).await?;

        // Connect to target peer's TCP server
        target_addr.set_port(self.state.constants.tcp_port);
        let mut stream = TcpStream::connect(target_addr).await?;

        // Send data kind
        stream
            .write_all(&[SyncKind::File(SyncFileKind::Transfer).as_u8()])
            .await?;

        // Send file name length (u64)
        stream
            .write_all(&u64::to_be_bytes(file.name.len() as u64))
            .await?;

        // Send file name
        stream.write_all(file.name.as_bytes()).await?;

        // Send file hash (32 bytes)
        let hash_bytes = hex::decode(&file.hash).unwrap();
        stream.write_all(&hash_bytes).await?;

        // Send file version
        stream.write_all(&u32::to_be_bytes(file.version)).await?;

        // Send file size (u64)
        let file_size = buffer.len() as u64;
        stream.write_all(&u64::to_be_bytes(file_size)).await?;

        // Send file contents
        stream.write_all(&buffer).await?;

        info!(
            "Sent file: {} ({} bytes) to {}",
            file.name, file_size, target_ip
        );
        Ok(())
    }

    pub async fn read_file(
        &self,
        src_ip: IpAddr,
        stream: &mut TcpStream,
    ) -> std::io::Result<ReceivedFile> {
        let mut name_len_buf = [0u8; 8];
        stream.read_exact(&mut name_len_buf).await?;
        let name_len = u64::from_be_bytes(name_len_buf) as usize;

        let mut file_name_buf = vec![0u8; name_len];
        stream.read_exact(&mut file_name_buf).await?;
        let file_name = String::from_utf8_lossy(&file_name_buf).into_owned();

        let mut hash_buf = [0u8; 32];
        stream.read_exact(&mut hash_buf).await?;
        let received_hash = hex::encode(hash_buf);

        let mut version_buf = [0u8; 4];
        stream.read_exact(&mut version_buf).await?;
        let file_version = u32::from_be_bytes(version_buf);

        let mut file_size_buf = [0u8; 8];
        stream.read_exact(&mut file_size_buf).await?;
        let file_size = u64::from_be_bytes(file_size_buf);

        let mut file_buf = vec![0u8; file_size as usize];
        stream.read_exact(&mut file_buf).await?;

        // Compute hash for corruption check
        let computed_hash = format!("{:x}", Sha256::digest(&file_buf));
        if computed_hash != received_hash {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Hash mismatch: data corruption detected",
            ));
        }

        Ok(ReceivedFile {
            name: file_name,
            version: file_version,
            size: file_size,
            contents: file_buf,
            hash: received_hash,
            from: src_ip,
        })
    }

    pub async fn save_file(
        &self,
        src_addr: SocketAddr,
        recv_file: &ReceivedFile,
    ) -> io::Result<()> {
        let file = FileInfo {
            name: recv_file.name.clone(),
            version: recv_file.version,
            hash: recv_file.hash.clone(),
            last_modified_by: Some(recv_file.from),
        };

        self.state.entry_manager.insert_file(file.clone());

        let original_path = self.state.constants.base_dir.join(&recv_file.name);
        let tmp_path = self.state.constants.tmp_dir.join(&recv_file.name);

        if let Some(parent) = tmp_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut tmp_file = FsFile::create(&tmp_path).await?;
        tmp_file.write_all(&recv_file.contents).await?;
        tmp_file.flush().await?;

        if let Some(parent) = original_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::rename(&tmp_path, &original_path).await?;

        self.send_metadata(&file, src_addr).await
    }

    pub async fn remove_file(&self, src_addr: SocketAddr, file_name: &str) -> io::Result<()> {
        let removed = self.state.entry_manager.remove_file(file_name);

        let path = self.state.constants.base_dir.join(file_name);
        let _ = fs::remove_file(path).await;

        self.send_metadata(&removed, src_addr).await
    }
}
