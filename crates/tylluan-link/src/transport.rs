use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Serialize error: {0}")]
    Serialize(String),
    #[error("Deserialize error: {0}")]
    Deserialize(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
}

#[async_trait]
pub trait MeshTransport: Send {
    async fn send(&mut self, data: &[u8]) -> Result<(), TransportError>;
    async fn receive(&mut self) -> Result<Vec<u8>, TransportError>;
    async fn close(&mut self) -> Result<(), TransportError>;
}

pub struct TcpTransport {
    stream: tokio::net::TcpStream,
    buf: Vec<u8>,
}

impl TcpTransport {
    pub async fn connect(addr: &str) -> Result<Self, TransportError> {
        let stream = tokio::net::TcpStream::connect(addr).await?;
        Ok(Self { stream, buf: vec![0u8; 4096] })
    }
}

#[async_trait]
impl MeshTransport for TcpTransport {
    async fn send(&mut self, data: &[u8]) -> Result<(), TransportError> {
        let len = data.len() as u32;
        let header = len.to_le_bytes();
        self.stream.write_all(&header).await?;
        self.stream.write_all(data).await?;
        self.stream.flush().await?;
        Ok(())
    }

    async fn receive(&mut self) -> Result<Vec<u8>, TransportError> {
        let mut header = [0u8; 4];
        self.stream.read_exact(&mut header).await?;
        let len = u32::from_le_bytes(header) as usize;
        if len > self.buf.len() {
            self.buf.resize(len, 0);
        }
        self.stream.read_exact(&mut self.buf[..len]).await?;
        Ok(self.buf[..len].to_vec())
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.stream.shutdown().await?;
        Ok(())
    }
}

pub struct InMemoryTransport {
    tx: Option<tokio::sync::mpsc::Sender<Vec<u8>>>,
    rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
}

pub fn in_memory_pair() -> (InMemoryTransport, InMemoryTransport) {
    let (tx1, rx1) = tokio::sync::mpsc::channel(64);
    let (tx2, rx2) = tokio::sync::mpsc::channel(64);
    (InMemoryTransport { tx: Some(tx1), rx: rx2 }, InMemoryTransport { tx: Some(tx2), rx: rx1 })
}

#[async_trait]
impl MeshTransport for InMemoryTransport {
    async fn send(&mut self, data: &[u8]) -> Result<(), TransportError> {
        match &self.tx {
            Some(tx) => tx.send(data.to_vec()).await
                .map_err(|_| TransportError::Io(io::Error::new(io::ErrorKind::BrokenPipe, "channel closed"))),
            None => Err(TransportError::Io(io::Error::new(io::ErrorKind::BrokenPipe, "transport closed"))),
        }
    }

    async fn receive(&mut self) -> Result<Vec<u8>, TransportError> {
        self.rx.recv().await
            .ok_or_else(|| TransportError::Io(io::Error::new(io::ErrorKind::UnexpectedEof, "channel closed")))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.tx = None;
        self.rx.close();
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportMessage {
    pub sender_id: String,
    pub payload: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_roundtrip() {
        let (mut a, mut b) = in_memory_pair();

        let send_task = tokio::spawn(async move {
            a.send(b"hello").await.unwrap();
            a.send(b"world").await.unwrap();
        });

        let recv1 = b.receive().await.unwrap();
        let recv2 = b.receive().await.unwrap();
        assert_eq!(recv1, b"hello");
        assert_eq!(recv2, b"world");

        send_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_in_memory_bidirectional() {
        let (mut a, mut b) = in_memory_pair();

        let h1 = tokio::spawn(async move {
            a.send(b"ping").await.unwrap();
            let msg = a.receive().await.unwrap();
            assert_eq!(msg, b"pong");
        });

        let h2 = tokio::spawn(async move {
            let msg = b.receive().await.unwrap();
            assert_eq!(msg, b"ping");
            b.send(b"pong").await.unwrap();
        });

        h1.await.unwrap();
        h2.await.unwrap();
    }

    #[tokio::test]
    async fn test_in_memory_close_detection() {
        let (mut a, mut b) = in_memory_pair();
        a.close().await.unwrap();
        let result = a.send(b"after close").await;
        assert!(result.is_err(), "sending after close should fail");
        let result = b.receive().await;
        assert!(result.is_err(), "reading from closed transport should fail");
    }
}
