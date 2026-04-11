//! Транспортный слой: WebSocket утилиты.

pub mod error;
pub mod websocket;

pub use error::TransportError;
pub use websocket::split_socket;
