pub enum NetworkError {
    SendingChannelHangup,
    ReceivingChannelHangup,
    NoMessageReceived,
}

pub type NetworkResult<T> = Result<T, NetworkError>;