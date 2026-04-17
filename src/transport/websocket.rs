//! Утилиты для работы с WebSocket.
//!
//! # Архитектурное решение
//!
//! Мы используем ограниченный канал (bounded channel — канал с фиксированным лимитом)
//! с ёмкостью 64 сообщения. Канал передаёт нативные кадры `Message`, а не строки.
//! Это позволяет:
//! 1. Отправлять управляющие кадры (Pong) напрямую, минуя сериализацию в JSON.
//! 2. Контролировать нагрузку: при переполнении канала сервер не накапливает данные в памяти,
//!    а принимает решение о закрытии соединения на уровне оркестратора.
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::debug;

// ─────────────────────────────────────────────────────────────
// Публичные типы
// ─────────────────────────────────────────────────────────────
/// Канал для отправки сообщений через WebSocket.
///
/// Теперь принимает `Message` (текст, пинг, понг, закрытие)
/// и имеет фиксированный размер буфера (64 элемента).
pub type WebSocketSender = mpsc::Sender<Message>;

// ─────────────────────────────────────────────────────────────
// Внутренние типы
// ─────────────────────────────────────────────────────────────
/// Поток входящих сообщений.
type IncomingStream = futures_util::stream::SplitStream<WebSocket>;

// ─────────────────────────────────────────────────────────────
// Публичные функции
// ─────────────────────────────────────────────────────────────
/// Разделяет WebSocket на независимые каналы отправки и получения.
///
/// # Возвращает
/// - `sender` — ограниченный канал для отправки любых `Message`
/// - `receiver` — поток для чтения входящих кадров
pub fn split_socket(
    socket: WebSocket,
) -> (WebSocketSender, IncomingStream) {
    // Разделяем сокет на писателя и читателя
    let (ws_sender, ws_receiver) = socket.split();

    // Создаём ограниченный канал ёмкостью 64
    let (tx, rx) = mpsc::channel::<Message>(64);

    // Запускаем фоновую задачу отправки
    spawn_sender_task(ws_sender, rx);

    (tx, ws_receiver)
}

// ─────────────────────────────────────────────────────────────
// Внутренние функции
// ─────────────────────────────────────────────────────────────
/// Фоновая задача: транслирует кадры из канала в WebSocket.
fn spawn_sender_task(
    mut ws_sender: futures_util::stream::SplitSink<WebSocket, Message>,
    mut rx: mpsc::Receiver<Message>,
) {
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = ws_sender.send(msg).await {
                // Ошибка отправки означает разрыв соединения или закрытие сокета
                debug!("Ошибка отправки WebSocket (peer disconnected): {}", e);
                break;
            }
        }
        debug!("Задача отправки WebSocket завершена");
    });
}

// ─────────────────────────────────────────────────────────────
// Вспомогательные функции (без изменений)
// ─────────────────────────────────────────────────────────────
/// Извлекает текст из сообщения WebSocket, если оно текстовое.
#[inline]
pub fn try_extract_text(msg: &Message) -> Option<String> {
    match msg {
        Message::Text(s) => Some(s.clone()),
        _ => None,
    }
}

/// Логирует тип входящего сообщения для отладки.
#[inline]
pub fn debug_log_message(msg: &Message) {
    match msg {
        Message::Text(s) => debug!("← Text ({} байт)", s.len()),
        Message::Binary(b) => debug!("← Binary ({} байт)", b.len()),
        Message::Ping(_) => debug!("← Ping"),
        Message::Pong(_) => debug!("← Pong"),
        Message::Close(f) => debug!("← Close: {:?}", f),
    }
}
