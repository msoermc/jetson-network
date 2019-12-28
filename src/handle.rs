use crate::prelude::{ReceivingMessage, SendingMessage, NetworkResult, NetworkError};
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;

pub struct NetworkHandle {
    receiver: mpsc::Receiver<ReceivingMessage>,
    sender: mpsc::Sender<SendingMessage>,
}

impl NetworkHandle {
    pub fn send(&mut self, message: SendingMessage) -> NetworkResult<()> {
        self.sender.send(message)
            .map(|_| ())
            .map_err(|_| NetworkError::SendingChannelHangup)
    }

    pub fn receive_non_blocking(&mut self) -> NetworkResult<ReceivingMessage> {
        self.receiver.try_recv().map_err(|err| match err {
            TryRecvError::Disconnected => NetworkError::ReceivingChannelHangup,
            TryRecvError::Empty => NetworkError::NoMessageReceived
        })
    }
}