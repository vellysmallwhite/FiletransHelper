use crate::transport::Transport;

#[derive(Clone, Debug, Default)]
pub struct QuicTransport;

impl Transport for QuicTransport {
    fn name(&self) -> &'static str {
        "quic"
    }
}
