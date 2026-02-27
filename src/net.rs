use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::types::{Message, Event, ValidatorId, ArchivedMessage};
use std::time::Instant;
use parking_lot::Mutex;
use rkyv::{check_archived_root, Deserialize};

#[derive(Clone, Debug)]
pub struct NetMetrics {
    pub ser_micros: u64,
    pub deser_micros: u64,
    pub bytes_sent: u64,
    pub bytes_recv: u64,
}

pub struct TcpNetwork {
    pub id: ValidatorId,
    pub listen_addr: String,
    pub peer_addrs: HashMap<ValidatorId, String>,
    pub metrics: Arc<Mutex<NetMetrics>>,
}

impl TcpNetwork {
    pub fn new(id: ValidatorId, listen_addr: String, peer_addrs: HashMap<ValidatorId, String>) -> Self {
        Self {
            id,
            listen_addr,
            peer_addrs,
            metrics: Arc::new(Mutex::new(NetMetrics {
                ser_micros: 0, deser_micros: 0, bytes_sent: 0, bytes_recv: 0
            })),
        }
    }

    pub async fn start(self, event_tx: mpsc::Sender<Event>) -> Arc<NetworkHandle> {
        let mut peer_senders = HashMap::new();
        let metrics = self.metrics.clone();

        for (peer_id, addr) in self.peer_addrs {
            let (tx, mut rx) = mpsc::channel::<Vec<u8>>(100_000);
            peer_senders.insert(peer_id, tx);
            let metrics_in = metrics.clone();
            
            tokio::spawn(async move {
                loop {
                    if let Ok(stream) = TcpStream::connect(&addr).await {
                        let _ = stream.set_nodelay(true);
                        let mut writer = BufWriter::new(stream);
                        while let Some(msg_bytes) = rx.recv().await {
                            let len = (msg_bytes.len() as u32).to_le_bytes();
                            if writer.write_all(&len).await.is_err() { break; }
                            if writer.write_all(&msg_bytes).await.is_err() { break; }
                            
                            // Only flush if no more messages are immediately available
                            if rx.is_empty() {
                                if writer.flush().await.is_err() { break; }
                            }
                            
                            {
                                let mut m = metrics_in.lock();
                                m.bytes_sent += msg_bytes.len() as u64;
                            }
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            });
        }

        let listener = TcpListener::bind(&self.listen_addr).await.unwrap();
        let event_tx_in = event_tx.clone();
        let metrics_in = metrics.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let _ = stream.set_nodelay(true);
                let tx = event_tx_in.clone();
                let m_in = metrics_in.clone();
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stream);
                    let mut len_buf = [0u8; 4];
                    loop {
                        if reader.read_exact(&mut len_buf).await.is_err() { break; }
                        let len = u32::from_le_bytes(len_buf) as usize;
                        let mut msg_buf = vec![0u8; len];
                        if reader.read_exact(&mut msg_buf).await.is_err() { break; }
                        
                        let start_deser = Instant::now();
                        let archived = check_archived_root::<Message>(&msg_buf).unwrap();
                        let msg: Message = archived.deserialize(&mut rkyv::Infallible).unwrap();
                        let deser_elapsed = start_deser.elapsed().as_micros() as u64;

                        {
                            let mut m = m_in.lock();
                            m.deser_micros += deser_elapsed;
                            m.bytes_recv += len as u64;
                        }

                        match msg {
                            Message::Vertex(v) => { let _ = tx.send(Event::VertexReceived(v)).await; }
                            Message::CoA(coa) => { let _ = tx.send(Event::CoAReceived(coa)).await; }
                            _ => {}
                        }
                    }
                });
            }
        });

        Arc::new(NetworkHandle {
            id: self.id,
            peer_senders,
            metrics,
        })
    }
}

pub struct NetworkHandle {
    pub id: ValidatorId,
    pub peer_senders: HashMap<ValidatorId, mpsc::Sender<Vec<u8>>>,
    pub metrics: Arc<Mutex<NetMetrics>>,
}

impl NetworkHandle {
    pub async fn broadcast_raw(&self, msg_bytes: Vec<u8>) {
        // Fire-and-forget: send to the MPMC channel, the spawned task handles the TCP write.
        for sender in self.peer_senders.values() {
            let _ = sender.send(msg_bytes.clone()).await;
        }
    }
    
    pub fn get_metrics(&self) -> NetMetrics {
        self.metrics.lock().clone()
    }
}
