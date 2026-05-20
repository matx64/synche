use crate::{
    application::AppState,
    application::network::transport::interface::TransportResult,
    domain::{EntryInfo, HandshakeData, TransportData},
    infra::network::tcp::{chunk::TRANSFER_CHUNK_SIZE, kind::TcpStreamKind},
};
use sha2::{Digest, Sha256};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};
use tracing::{info, warn};

pub struct TcpSender {
    state: Arc<AppState>,
}

impl TcpSender {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub async fn send_data(&self, target: IpAddr, data: TransportData) -> TransportResult<()> {
        let kind = TcpStreamKind::from(&data);

        match data {
            TransportData::HandshakeSyn(hs_data) | TransportData::HandshakeAck(hs_data) => {
                self.send_handshake(target, hs_data, kind).await
            }
            TransportData::Metadata(entry) => self.send_metadata(target, entry).await,
            TransportData::Request(entry) => self.send_request(target, entry).await,
            TransportData::Transfer(entry) => self.send_entry(target, entry).await,
        }
    }

    async fn send_handshake(
        &self,
        target: IpAddr,
        hs_data: HandshakeData,
        kind: TcpStreamKind,
    ) -> TransportResult<()> {
        let socket = SocketAddr::new(target, self.state.ports().transport);
        let mut stream = TcpStream::connect(socket).await?;

        let contents = serde_json::to_vec(&hs_data)?;

        info!(kind = kind.to_string(), target = ?target, "[⬆️  SEND]");

        stream.write_all(self.state.local_id().as_bytes()).await?;
        stream.write_all(&[kind as u8]).await?;
        stream
            .write_all(&(contents.len() as u32).to_be_bytes())
            .await?;
        stream.write_all(&contents).await?;

        Ok(())
    }

    async fn send_metadata(&self, target: IpAddr, entry: EntryInfo) -> TransportResult<()> {
        let socket = SocketAddr::new(target, self.state.ports().transport);
        let mut stream = TcpStream::connect(socket).await?;

        let kind = TcpStreamKind::Metadata;
        let contents = serde_json::to_vec(&entry)?;

        info!(kind = kind.to_string(), target = ?target, entry_name = ?&entry.name, "[⬆️  SEND]");

        stream.write_all(self.state.local_id().as_bytes()).await?;
        stream.write_all(&[kind as u8]).await?;
        stream
            .write_all(&u32::to_be_bytes(contents.len() as u32))
            .await?;
        stream.write_all(&contents).await?;
        Ok(())
    }

    async fn send_request(&self, target: IpAddr, entry: EntryInfo) -> TransportResult<()> {
        let socket = SocketAddr::new(target, self.state.ports().transport);
        let mut stream = TcpStream::connect(socket).await?;

        let kind = TcpStreamKind::Request;
        let contents = serde_json::to_vec(&entry)?;

        info!(kind = kind.to_string(), target = ?target, entry_name = ?&entry.name, "[⬆️  SEND]");

        stream.write_all(self.state.local_id().as_bytes()).await?;
        stream.write_all(&[kind as u8]).await?;
        stream
            .write_all(&u32::to_be_bytes(contents.len() as u32))
            .await?;
        stream.write_all(&contents).await?;
        Ok(())
    }

    async fn send_entry(&self, target: IpAddr, entry: EntryInfo) -> TransportResult<()> {
        let socket = SocketAddr::new(target, self.state.ports().transport);
        let mut stream = TcpStream::connect(socket).await?;

        let path = entry.name.to_canonical(self.state.home_path());
        // Open the file first, then derive the wire size from the same handle
        // we will read from. Reading metadata via `fs::metadata` and then
        // opening separately would race: the file could be replaced or
        // truncated between the two syscalls, so the size advertised on the
        // wire would not match the bytes we then stream.
        let mut file = File::open(&path).await?;
        let entry_size = file.metadata().await?.len();

        let kind = TcpStreamKind::Transfer;
        let metadata_json = serde_json::to_vec(&entry)?;

        info!(kind = kind.to_string(), target = ?target, entry_name = ?&entry.name, "[⬆️  SEND]");

        // Write self peer id
        stream.write_all(self.state.local_id().as_bytes()).await?;

        // Write sync kind
        stream.write_all(&[kind as u8]).await?;

        // Write metadata json size
        stream
            .write_all(&u32::to_be_bytes(metadata_json.len() as u32))
            .await?;

        // Write metadata json
        stream.write_all(&metadata_json).await?;

        // Write entry size
        stream.write_all(&u64::to_be_bytes(entry_size)).await?;

        // Stream entry contents in chunks
        let computed_hash =
            Self::stream_file_to(&mut file, &mut stream, entry_size, TRANSFER_CHUNK_SIZE).await?;

        if let Some(expected) = entry.hash.as_deref()
            && computed_hash != expected
        {
            warn!(
                entry_name = ?&entry.name,
                "⚠️  File changed during transfer; receiver will reject by hash mismatch.",
            );
        }

        Ok(())
    }

    /// Stream exactly `total` bytes from `file` to `writer` in `chunk_size` chunks,
    /// returning the hex-encoded SHA-256 of the bytes streamed. If the file is
    /// shorter than `total`, the remainder is zero-padded so the wire framing
    /// matches the size advertised in the header.
    pub(super) async fn stream_file_to<R, W>(
        file: &mut R,
        writer: &mut W,
        total: u64,
        chunk_size: usize,
    ) -> TransportResult<String>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; chunk_size];
        let mut remaining = total;

        while remaining > 0 {
            let want = remaining.min(chunk_size as u64) as usize;
            let mut filled = 0;
            while filled < want {
                match file.read(&mut buf[filled..want]).await? {
                    0 => break,
                    n => filled += n,
                }
            }
            if filled < want {
                // File shrank mid-transfer: pad with zeros to honour the
                // advertised entry_size. Hash will diverge and the receiver
                // will reject.
                buf[filled..want].fill(0);
            }
            hasher.update(&buf[..want]);
            writer.write_all(&buf[..want]).await?;
            remaining -= want as u64;
        }

        Ok(format!("{:x}", hasher.finalize()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::network::transport::interface::TransportError;
    use std::io::Cursor;

    fn ok<T>(res: TransportResult<T>) -> T {
        match res {
            Ok(v) => v,
            Err(TransportError::Failure(m)) => panic!("{m}"),
        }
    }

    #[tokio::test]
    async fn stream_file_to_writes_exact_bytes_and_hashes_them() {
        let payload: Vec<u8> = (0..200u32).map(|i| (i % 256) as u8).collect();
        let mut src = Cursor::new(payload.clone());
        let mut dst: Vec<u8> = Vec::new();

        let hash =
            ok(TcpSender::stream_file_to(&mut src, &mut dst, payload.len() as u64, 16).await);

        assert_eq!(dst, payload);
        assert_eq!(hash, format!("{:x}", Sha256::digest(&payload)));
    }

    #[tokio::test]
    async fn stream_file_to_handles_exact_chunk_boundary() {
        let payload: Vec<u8> = (0..64u32).map(|i| (i % 256) as u8).collect();
        let mut src = Cursor::new(payload.clone());
        let mut dst: Vec<u8> = Vec::new();

        ok(TcpSender::stream_file_to(&mut src, &mut dst, payload.len() as u64, 16).await);

        assert_eq!(dst, payload);
    }

    #[tokio::test]
    async fn stream_file_to_pads_when_source_shrinks() {
        let payload = vec![0xABu8; 10];
        let mut src = Cursor::new(payload);
        let mut dst: Vec<u8> = Vec::new();

        ok(TcpSender::stream_file_to(&mut src, &mut dst, 24, 8).await);

        assert_eq!(dst.len(), 24);
        assert_eq!(&dst[..10], &[0xAB; 10]);
        assert_eq!(&dst[10..], &[0u8; 14]);
    }
}
