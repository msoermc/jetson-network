use async_std::sync::{Sender, Receiver};
use crate::messages::{SendingMessage, ReceivingMessage};
use async_std::net::{ToSocketAddrs, SocketAddr, TcpListener, TcpStream};
use async_std::sync::{Arc, RwLock, Mutex};
use async_tungstenite::{WebSocketStream, accept_async};
use async_tungstenite::tungstenite::Message;
use async_std::task;
use futures::sink::SinkExt;
use log::{info, warn, error, debug, trace};
use futures::StreamExt;
use std::time::Duration;
use futures::stream::{SplitSink, SplitStream};

mod messages;
mod errors;

struct DuplexStreamMutexPair<S, M> {
    pub writer: Mutex<SplitSink<S, M>>,
    pub reader: Mutex<SplitStream<S>>,
}

impl<S, M> DuplexStreamMutexPair<S, M> {
    pub fn new(writer: SplitSink<S, M>, reader: SplitStream<S>) -> Self {
        Self {
            writer: Mutex::new(writer),
            reader: Mutex::new(reader),
        }
    }
}

type WebSocketStreamAggregator =
Arc<
    RwLock<
        Vec<
            RwLock<
                DuplexStreamMutexPair<
                    WebSocketStream<TcpStream>,
                    Message
                >
            >
        >
    >
>;

pub mod prelude {
    pub use crate::errors::{NetworkError, NetworkResult};
    pub use crate::messages::{SendingMessage, ReceivingMessage, ReceivingMessagePayload, SendingMessagePayload};
}

pub async fn launch(sender: Sender<ReceivingMessage>, receive: Receiver<SendingMessage>, addr: &str) {
    let addr: SocketAddr = addr.to_socket_addrs().await.unwrap().next().unwrap();
    let listener: TcpListener = TcpListener::bind(&addr).await.unwrap();

    let streams: WebSocketStreamAggregator = Arc::new(RwLock::new(Vec::with_capacity(10)));

    task::spawn(process_streams(listener, streams.clone()));
    task::spawn(process_reads(sender, streams.clone()));
    task::spawn(process_writes(receive, streams));
}

/// Asynchronously reads
async fn process_reads(sender: Sender<ReceivingMessage>, streams: WebSocketStreamAggregator) {
    loop {
        let streams = streams.read().await;
        for stream_rw_lock in streams.iter() {
            debug!("R: R-Locking duplex");
            let stream = stream_rw_lock.read().await;
            debug!("R: Locking reader");
            let mut reader = stream.reader.lock().await;

            debug!("R: Checking for new message");
            // TODO remove unwraps
            // TODO make it so that a single unresponsive stream get's ignored
            let message: Message = reader.next().await.unwrap().unwrap();
            let message_string = message.into_text().unwrap();
            match serde_json::from_str(&message_string) {
                Ok(received) => sender.send(received).await,
                Err(e) => error!("Received invalid message {:?}", e),
            };
        }
    }
}

/// Not currently working
async fn process_writes(receiver: Receiver<SendingMessage>, streams: WebSocketStreamAggregator) {
    loop {
        let msg: SendingMessage = receiver.recv().await.expect("Channel hangup");
        let streams = streams.read().await;
        for stream_rw_lock in streams.iter() {
            debug!("W: R-Locking duplex");
            let stream = stream_rw_lock.read().await;
            debug!("W: Locking writer");
            let mut writer = stream.writer.lock().await;
            let msg = Message::Text(serde_json::to_string(&msg).unwrap());
            debug!("Sent: {:?}", msg);
            writer.send(msg).await.unwrap();
            writer.flush().await.unwrap();
        }
    }
}

async fn process_streams(listener: TcpListener, streams: WebSocketStreamAggregator) {
    while let Ok((stream, _)) = listener.accept().await {
        if let Ok(ws) = accept_async(stream).await {
            let (writer, reader) = ws.split();
            let duplex = DuplexStreamMutexPair::new(writer, reader);
            debug!("S: W-Locking streams");
            streams.write().await.push(RwLock::new(duplex));
            debug!("S: Added new stream");
        }
    }
}