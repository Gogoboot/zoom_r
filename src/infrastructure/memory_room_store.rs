//! Реализация хранилища комнат в памяти (DashMap).
use std::sync::Arc;
use dashmap::DashMap;
use async_trait::async_trait;
use crate::domain::Room;
use crate::infrastructure::{RoomRepository, InfraError};

#[derive(Clone)]
pub struct MemoryRoomStore {
    rooms: Arc<DashMap<String, Room>>,
}

impl MemoryRoomStore {
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(DashMap::new()),
        }
    }
}

#[async_trait]
impl RoomRepository for MemoryRoomStore {
    async fn insert(&self, room: Room) -> Result<(), InfraError> {
        self.rooms.insert(room.id.clone(), room);
        Ok(())
    }

    async fn get(&self, room_id: &str) -> Result<Option<Room>, InfraError> {
        Ok(self.rooms.get(room_id).map(|r| r.clone()))
    }

    async fn get_mut(&self, room_id: &str) -> Result<Option<Room>, InfraError> {
        Ok(self.rooms.get(room_id).map(|r| r.clone()))
    }

    async fn remove(&self, room_id: &str) -> Result<Option<Room>, InfraError> {
        Ok(self.rooms.remove(room_id).map(|(_, r)| r))
    }

    async fn contains(&self, room_id: &str) -> Result<bool, InfraError> {
        Ok(self.rooms.contains_key(room_id))
    }

    /// Атомарная проверка и удаление. DashMap гарантирует, что операция
    /// выполняется под внутренней блокировкой, без гонок состояний.
    async fn remove_if_empty(&self, room_id: &str) -> Result<(), InfraError> {
        // remove_if возвращает Some, если условие true и запись удалена
        self.rooms.remove_if(room_id, |_, room| room.is_empty());
        Ok(())
    }
}
