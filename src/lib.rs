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
use futures::stream::{SplitSink, SplitStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::ops::Index;

mod messages;
mod errors;

pub mod prelude {
    pub use crate::errors::{NetworkError, NetworkResult};
    pub use crate::messages::{SendingMessage, ReceivingMessage, ReceivingMessagePayload, SendingMessagePayload};
}

pub const NEXT_CON_ID: AtomicUsize = AtomicUsize::new(0);

struct DuplexStreamMutexPair<S, M> {
    pub writer: Mutex<SplitSink<S, M>>,
    pub reader: Mutex<SplitStream<S>>,
    pub is_active: AtomicBool,
    pub id: usize,
}

impl<S, M> DuplexStreamMutexPair<S, M> {
    pub fn new(writer: SplitSink<S, M>, reader: SplitStream<S>) -> Self {
        Self {
            writer: Mutex::new(writer),
            reader: Mutex::new(reader),
            is_active: AtomicBool::new(true),
            id: NEXT_CON_ID.fetch_add(1, Ordering::AcqRel),
        }
    }
}

type WebSocketStreamAggregator =
Arc<
    RwLock<
        Vec<
            Arc<
                DuplexStreamMutexPair<
                    WebSocketStream<TcpStream>,
                    Message
                >
            >
        >
    >
>;


pub async fn launch(sender: Sender<ReceivingMessage>, receive: Receiver<SendingMessage>, addr: &str) {
    let addr: SocketAddr = addr.to_socket_addrs().await.unwrap().next().unwrap();
    let listener: TcpListener = TcpListener::bind(&addr).await.unwrap();

    let streams: WebSocketStreamAggregator = Arc::new(RwLock::new(Vec::with_capacity(8)));

    task::spawn(process_streams(sender, listener, streams.clone()));
    task::spawn(process_writes(receive, streams));
}

async fn process_reader(sender: Sender<ReceivingMessage>,
                        duplex: Arc<DuplexStreamMutexPair<WebSocketStream<TcpStream>, Message>>) {
    loop {
        if duplex.is_active.load(Ordering::Relaxed) {
            let mut reader = duplex.reader.lock().await;
            if let Some(Ok(message)) = reader.next().await {
                let message_string = message.into_text().unwrap();
                match serde_json::from_str(&message_string) {
                    Ok(received) => sender.send(received).await,
                    Err(e) => error!("Received invalid message {:?}", e),
                };
            } else {
                // let everyone know it's dead
                duplex.is_active.store(false, Ordering::SeqCst);
                return; // this connection is dead, nope out of here
            }
        } else {
            return; // this connection is dead, nope out of here
        }
    }
}

// TODO handle errors
async fn process_writes(receiver: Receiver<SendingMessage>, streams: WebSocketStreamAggregator) {
    loop {
        let msg: SendingMessage = receiver.recv().await.expect("Channel hangup");
        let streams_guard = streams.read().await;
        for stream in streams_guard.iter() {
            if stream.is_active.load(Ordering::Relaxed) {
                let msg_string = serde_json::to_string(&msg).expect("Failed to convert item to json");
                let msg = Message::Text(msg_string);
                task::spawn(process_sendoff(stream.clone(), msg));
            } else {
                task::spawn(delete_inactive_connection(streams.clone(), stream.id));
            }
        }
    }
}

async fn delete_inactive_connection(streams: WebSocketStreamAggregator, id: usize) {
    let mut streams_guard = streams.write().await;

    for i in 0..(streams_guard.len()) {
        if streams_guard.index(i).id == id {
            streams_guard.remove(i);
            break;
        }
    }
}

async fn process_sendoff(duplex: Arc<DuplexStreamMutexPair<WebSocketStream<TcpStream>, Message>>,
                         msg: Message) {
    let mut writer = duplex.writer.lock().await;
    let mut ok =  true;

    let send = writer.send(msg).await.is_ok();

    ok &= send;

    if send {
        ok &= writer.flush().await.is_ok();
    }

    if !ok {
        // let everyone know it's dead
        duplex.is_active.store(false, Ordering::SeqCst);
    }
}

async fn process_streams(sender: Sender<ReceivingMessage>,
                         listener: TcpListener,
                         streams: WebSocketStreamAggregator) {
    while let Ok((stream, _)) = listener.accept().await {
        if let Ok(ws) = accept_async(stream).await {
            let (writer, reader) = ws.split();
            let duplex = DuplexStreamMutexPair::new(writer, reader);
            let connection = Arc::new(duplex);

            streams.write().await.push(connection.clone());
            task::spawn(process_reader(sender.clone(), connection));
        }
    }
}