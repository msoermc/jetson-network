use std::sync::mpsc::{Sender, Receiver};
use crate::messages::{SendingMessage, ReceivingMessage};
use async_std::net::{ToSocketAddrs, SocketAddr, TcpListener, TcpStream};
use async_std::sync::{Arc, RwLock, Mutex};
use async_tungstenite::{WebSocketStream, accept_async};
use async_tungstenite::tungstenite::Message;
use async_std::task;

use log::{info, warn, error, debug, trace};
use futures::StreamExt;

mod messages;
mod errors;
mod handle;

pub mod prelude {
    pub use crate::handle::NetworkHandle;
    pub use crate::errors::{NetworkError, NetworkResult};
    pub use crate::messages::{SendingMessage, ReceivingMessage, ReceivingMessagePayload};
}

pub async fn launch(sender: Sender<ReceivingMessage>, receive: Receiver<SendingMessage>) {
    let addr: SocketAddr = "0.0.0.0:1776".to_socket_addrs().await.unwrap().next().unwrap();
    let listener: TcpListener = TcpListener::bind(&addr).await.unwrap();

    let streams = Arc::new(RwLock::new(Vec::with_capacity(10)));

    task::spawn(process_streams(listener, streams.clone()));

    task::spawn(process_reads(sender, streams));
}

pub async fn process_reads(sender: Sender<ReceivingMessage>,
                           streams: Arc<RwLock<Vec<Mutex<WebSocketStream<TcpStream>>>>>) {
    loop {
        let streams = streams.read().await;
        for stream_mutex in streams.iter() {
            let mut stream = stream_mutex.lock().await;
            // TODO remove unwraps
            let message: Message = stream.next().await.unwrap().unwrap();
            match serde_json::from_str(&message.into_text().unwrap()) {
                Ok(received) => sender.send(received).unwrap(),
                Err(e) => error!("Received invalid message {:?}", e),
            };
        }
    }
}

pub async fn process_writes(receiver: Receiver<SendingMessage>,
                            streams: Arc<RwLock<Vec<Mutex<WebSocketStream<TcpStream>>>>>) {
    loop {
        let streams = streams.read().await;
        for stream_mutex in streams.iter() {
            let mut stream = stream_mutex.lock().await;
            todo!()
        }
    }
}

pub async fn process_streams(listener: TcpListener,
                             streams: Arc<RwLock<Vec<Mutex<WebSocketStream<TcpStream>>>>>) {
    while let Ok((stream, _)) = listener.accept().await {
        if let Ok(ws) = accept_async(stream).await {
            streams.write().await.push(Mutex::new(ws))
        }
    }
}