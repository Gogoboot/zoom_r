//! Трейт для хранилища комнат (Dependency Inversion).

use async_trait::async_trait;
use crate::domain::Room;
use crate::infrastructure::InfraError;

#[async_trait]
pub trait RoomRepository: Send + Sync {
    async fn insert(&self, room: Room) -> Result<(), InfraError>;
    async fn get(&self, room_id: &str) -> Result<Option<Room>, InfraError>;
    async fn get_mut(&self, room_id: &str) -> Result<Option<Room>, InfraError>; // упрощённо, возвращаем владение
    async fn remove(&self, room_id: &str) -> Result<Option<Room>, InfraError>;
    async fn contains(&self, room_id: &str) -> Result<bool, InfraError>;
}

// Примечание: get_mut возвращает Room целиком, потому что мы не можем возвращать ссылку в async трейте.
// В реальности можно использовать блокировки или возвращать специальный Guard, но для простоты так.
