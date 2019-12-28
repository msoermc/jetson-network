use serde::{Serialize, Deserialize};

#[derive(Deserialize, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub struct ReceivingMessage {
    pub payload: ReceivingMessagePayload
}

#[derive(Deserialize, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub enum ReceivingMessagePayload {
    Drive {
        left: u8,
        right: u8
    }
}

#[derive(Serialize, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub struct SendingMessage {

}