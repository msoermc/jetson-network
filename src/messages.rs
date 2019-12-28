use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Debug)]
pub struct ReceivingMessage {
    pub payload: ReceivingMessagePayload
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Debug)]
pub enum ReceivingMessagePayload {
    Drive {
        left: u8,
        right: u8
    }
}

#[derive(Serialize, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub struct SendingMessage {

}