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

pub mod prelude {
    pub use crate::messages::{SendingMessage, ReceivingMessage, ReceivingMessagePayload, SendingMessagePayload};
}

// don't make this `const`, will result in unexpected behavior
static NEXT_CON_ID: AtomicUsize = AtomicUsize::new(0);

struct Connection<S = WebSocketStream<TcpStream>, M = Message> {
    pub writer: Mutex<SplitSink<S, M>>,
    pub reader: Mutex<SplitStream<S>>,
    /// A flag used to indicate if a connection is dead
    /// Dead collections are deleted
    pub is_active: AtomicBool,
    /// A unique identifier for our connections, used so that we can identify
    /// connections in the `ConnectionsList`
    pub id: usize,
}

impl<S, M> Connection<S, M> {
    pub fn new(writer: SplitSink<S, M>, reader: SplitStream<S>) -> Self {
        Self {
            writer: Mutex::new(writer),
            reader: Mutex::new(reader),
            is_active: AtomicBool::new(true),
            id: NEXT_CON_ID.fetch_add(1, Ordering::AcqRel),
        }
    }
}

/// Typedef for the list of connections
type ConnectionList = Arc<RwLock<Vec<Arc<Connection>>>>;

/// Asynchronously launch the network
pub async fn launch(sender: Sender<ReceivingMessage>, receive: Receiver<SendingMessage>, addr: &str) {
    info!("Launching network");

    // parse the address
    let addr: SocketAddr = addr
        .to_socket_addrs()
        .await
        .expect("Failed to parse socket address")
        .next()
        .expect("Failed to iterate through awaited socket address future");

    // bind the tcp listener to the address
    // keep retrying so that we are certain it succeeds, although it shouldn't fail in theory
    let listener: TcpListener = loop {
        match TcpListener::bind(&addr).await {
            Ok(l) => break l,
            Err(e) => error!("Failed to instantiate tcp listener with error {}", e),
        }
    };

    // create the collection of streams
    let streams: ConnectionList = Arc::new(RwLock::new(Vec::with_capacity(8)));

    // kick off the tasks for accepting connections and writing messages
    task::spawn(accept_connections(sender, listener, streams.clone()));
    task::spawn(process_writes(receive, streams));
}

/// Task for accepting oncoming connections
async fn accept_connections(sender: Sender<ReceivingMessage>, listener: TcpListener, streams: ConnectionList) {
    // accept a connection
    while let Ok((stream, _)) = listener.accept().await {
        info!("Received new TCP connection");
        // try and convert the connection to a websocket
        if let Ok(ws) = accept_async(stream).await {
            debug!("Successfully upgraded connection to a websocket");

            // split the duplex stream
            let (writer, reader) = ws.split();

            // create our custom duplex structure
            let duplex = Connection::new(writer, reader);

            // toss that into a shared ownership object
            let connection = Arc::new(duplex);

            // store the connection
            streams.write().await.push(connection.clone());
            trace!("Stored connection");

            // kick off a reader task
            task::spawn(process_reader(sender.clone(), connection));
            trace!("Spawned reader for connection")
        } else {
            error!("Received tcp connection, but failed to upgrade to websocket")
        }
    }

    error!("TCP Listener died!")
}

/// Reader task for reading from a single connection
async fn process_reader(sender: Sender<ReceivingMessage>, duplex: Arc<Connection>) {
    loop {
        // check if the connection is active
        if duplex.is_active.load(Ordering::Relaxed) {

            // lock the reader
            let mut reader = duplex.reader.lock().await;

            // attempt to read a message
            if let Some(Ok(message)) = reader.next().await {

                // get string form
                match message.into_text() {
                    Ok(message_string) => {
                        // parse json
                        match serde_json::from_str(&message_string) {
                            Ok(received) => {
                                info!("Received: {:?}", received);
                                sender.send(received).await
                            },
                            Err(e) => error!("Received invalid message {:?}", e),
                        };
                    }
                    Err(_e) => {
                        warn!("Received non-text message")
                    }
                }
            } else { // if we failed to get a message
                info!("Network: Reader marking connection as inactive");
                // mark the connection as inactive
                duplex.is_active.store(false, Ordering::Relaxed);
                return; // this connection is dead, nope out of here
            }
        } else {
            return; // this connection is dead, nope out of here
        }
    }
}

/// Writer task for writing messages to streams
async fn process_writes(receiver: Receiver<SendingMessage>, streams: ConnectionList) {
    loop {
        // get next message from channel
        let next_message_result: Option<SendingMessage> = receiver.recv().await;
        if let Some(msg) = next_message_result {
            debug!("Sending message {:?}", msg);

            // lock stream collection
            let streams_guard = streams.read().await;

            // traverse the streams and attempt to send the message to each one
            for stream in streams_guard.iter() {
                // check if the stream is active
                if stream.is_active.load(Ordering::Relaxed) {
                    // parse json
                    let msg_string = serde_json::to_string(&msg)
                        .expect("Failed to convert item to json");

                    // create the websocket message
                    let msg = Message::Text(msg_string);

                    // spawn a task to sendoff the message asynchronously
                    task::spawn(process_sendoff(stream.clone(), msg));
                } else {
                    // if the stream is inactive, yeet it out of here asynchronously
                    task::spawn(delete_inactive_connection(streams.clone(), stream.id));
                }
            }
        } else {
            // exit this process so that hopefully things can die gracefully if channel hangs up
            return;
        }
    }
}

/// Deletes an inactive connection
async fn delete_inactive_connection(streams: ConnectionList, id: usize) {
    // lock streams for writing
    let mut streams_guard = streams.write().await;

    // traverse the streams in search of the one with the matching id
    for i in 0..(streams_guard.len()) {
        // check if the id matches
        if streams_guard.index(i).id == id {
            // yeet that boi
            streams_guard.remove(i);
            debug!("Network: Removed inactive connection");
            break;
        }
    }
}

/// Send a message to connection endpoint
async fn process_sendoff(duplex: Arc<Connection>, msg: Message) {
    // lock the writer
    let mut writer = duplex.writer.lock().await;

    let mut is_operation_successful = true;

    // try and send the message
    let send = writer.send(msg).await.is_ok();

    // update the success value
    is_operation_successful &= send;

    // flush the stream if the send was successful
    if send {
        // update the success value
        is_operation_successful &= writer.flush().await.is_ok();
    }

    // mark the connection as inactive if the operation failed
    if !is_operation_successful {
        debug!("Network writer marked connection as inactive");
        duplex.is_active.store(false, Ordering::SeqCst);
    }
}