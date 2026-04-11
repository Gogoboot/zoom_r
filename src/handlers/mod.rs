//! Обработчики сообщений (чистая логика, без знания о транспорте).

pub mod error;
pub mod room;
pub mod webrtc;

pub use error::HandlerError;
pub use room::{handle_create_room, handle_join_room, handle_leave_room};
pub use webrtc::{handle_offer, handle_answer, handle_ice_candidate};
