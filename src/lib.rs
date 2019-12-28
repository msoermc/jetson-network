use async_std::sync::{Sender, Receiver};
use crate::messages::{SendingMessage, ReceivingMessage};
use async_std::net::{ToSocketAddrs, SocketAddr, TcpListener, TcpStream};
use async_std::sync::{Arc, RwLock, Mutex};
use async_tungstenite::{WebSocketStream, accept_async};
use async_tungstenite::tungstenite::Message;
use async_std::task;
use futures::sink::SinkExt;
use log::{info, warn, error, debug, trace};
use futures::{StreamExt};
use std::time::Duration;

mod messages;
mod errors;

pub mod prelude {
    pub use crate::errors::{NetworkError, NetworkResult};
    pub use crate::messages::{SendingMessage, ReceivingMessage, ReceivingMessagePayload, SendingMessagePayload};
}

pub async fn launch(sender: Sender<ReceivingMessage>, receive: Receiver<SendingMessage>,
                    address: &str) {
    let addr: SocketAddr = address.to_socket_addrs().await.unwrap().next().unwrap();
    let listener: TcpListener = TcpListener::bind(&addr).await.unwrap();

    let streams = Arc::new(RwLock::new(Vec::with_capacity(10)));

    task::spawn(process_streams(listener, streams.clone()));
    task::spawn(process_reads(sender, streams.clone()));
    task::spawn(process_writes(receive, streams));
}

pub async fn process_reads(sender: Sender<ReceivingMessage>,
                           streams: Arc<RwLock<Vec<Mutex<WebSocketStream<TcpStream>>>>>) {
    loop {
        let streams = streams.read().await;
        for stream_mutex in streams.iter() {
            println!("Locking R");
            let mut stream = stream_mutex.lock().await;
            println!("Locked R");
            // TODO remove unwraps
            let message: Message = stream.next().await.unwrap().unwrap();
            drop(stream);
            match serde_json::from_str(&message.into_text().unwrap()) {
                Ok(received) => sender.send(received).await,
                Err(e) => error!("Received invalid message {:?}", e),
            };
        }
        drop(streams)
    }
}

pub async fn process_writes(receiver: Receiver<SendingMessage>,
                            streams: Arc<RwLock<Vec<Mutex<WebSocketStream<TcpStream>>>>>) {
    loop {
        let msg: SendingMessage = receiver.recv().await.expect("Channel hangup");
        let streams = streams.read().await;
        for stream_mutex in streams.iter() {
            println!("Locking W");
            let mut stream = stream_mutex.lock().await;
            println!("Locked W");
            let msg = Message::Text(serde_json::to_string(&msg).unwrap());
            println!("msg: {:?}", msg);
            stream.send(msg).await.unwrap();
            stream.flush().await.unwrap();
            drop(stream);
        }
        drop(streams)
    }
}

pub async fn process_streams(listener: TcpListener,
                             streams: Arc<RwLock<Vec<Mutex<WebSocketStream<TcpStream>>>>>) {
    while let Ok((stream, _)) = listener.accept().await {
        if let Ok(ws) = accept_async(stream).await {
            streams.write().await.push(Mutex::new(ws));
        }
    }
}