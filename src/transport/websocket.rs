//! Утилиты для работы с WebSocket.
//!
//! # Архитектурное решение
//!
//! Мы используем `mpsc::UnboundedSender<String>` вместо нативного `Sink<Message>` по трём причинам:
//!
//! 1. **Удобство**: отправка не требует `await`, можно вызывать из синхронного контекста
//! 2. **Клонирование**: `UnboundedSender` клонируется дёшево (счётчик ссылок), удобно передавать в обработчики
//! 3. **Типобезопасность**: ограничиваем отправку только текстовыми сообщениями (наш протокол использует только JSON)
//!
//! # Ограничения
//!
//! - Поддерживаются только текстовые сообщения (`Message::Text`)
//! - При разрыве соединения отправка вернёт ошибку — обрабатывайте её на уровне вызывающего кода

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::debug;

// ─────────────────────────────────────────────────────────────
// Публичные типы (только то, что нужно внешнему коду)
// ─────────────────────────────────────────────────────────────

/// Канал для отправки текстовых сообщений через WebSocket.
///
/// # Пример
///
/// ```rust
/// # use signaling_server::transport::WebSocketSender;
/// fn notify_all(recipients: &[WebSocketSender], msg: &str) {
///     for tx in recipients {
///         let _ = tx.send(msg.to_string()); // не требует await
///     }
/// }
/// ```
pub type WebSocketSender = mpsc::UnboundedSender<String>;

// ─────────────────────────────────────────────────────────────
// Внутренние типы (не экспортируются)
// ─────────────────────────────────────────────────────────────

/// Внутренний тип для потока входящих сообщений.
///
/// Не экспортируется, чтобы не привязывать внешний код к конкретной
/// реализации `futures_util::stream`. Если понадобится абстрагироваться —
/// можно будет добавить трейт `MessageStream` позже.
type IncomingStream = futures_util::stream::SplitStream<WebSocket>;

// ─────────────────────────────────────────────────────────────
// Публичные функции
// ─────────────────────────────────────────────────────────────

/// Разделяет WebSocket на независимые каналы отправки и получения.
///
/// # Что делает
///
/// 1. Создаёт `mpsc`-канал для удобной отправки **текстовых** сообщений
/// 2. Запускает фоновую задачу, которая транслирует: `String` → `Message::Text` → WebSocket
/// 3. Возвращает пару `(sender, receiver)`:
///    - `sender` — для отправки из любого места (оркестратор, обработчики)
///    - `receiver` — для чтения входящих сообщений в главном цикле
///
/// # Почему так?
///
/// - **Отправка**: `UnboundedSender<String>` проще, чем `Sink<Message>` — не нужно `await`, можно клонировать
/// - **Получение**: оставляем нативный `SplitStream`, чтобы не терять доступ к пингам/закрытиям
/// - **Фоновая задача**: изолирует логику трансляции, основной поток не блокируется
///
/// # Обработка ошибок
///
/// - Если `send()` вернёт `Err` — получатель отключился, удалите его из комнаты
/// - Если `receiver.next()` вернёт `None` — соединение закрыто, завершите обработку
///
/// # Пример
///
/// ```rust
/// # use axum::extract::ws::WebSocket;
/// # use signaling_server::transport::split_socket;
/// # async fn handle(socket: WebSocket) {
/// let (tx, mut rx) = split_socket(socket);
///
/// // Отправка (не требует await)
/// let _ = tx.send(r#"{"type":"hello"}"#.to_string());
///
/// // Получение
/// while let Some(Ok(msg)) = rx.next().await {
///     // обработка msg...
/// }
/// # }
/// ```
pub fn split_socket(
    socket: WebSocket,
) -> (WebSocketSender, IncomingStream) {
    let (ws_sender, ws_receiver) = socket.split();
    let (tx, rx) = mpsc::unbounded_channel::<String>();

    // Запускаем фоновую задачу для трансляции: String → WebSocket::Text
    spawn_sender_task(ws_sender, rx);

    (tx, ws_receiver)
}

// ─────────────────────────────────────────────────────────────
// Внутренние функции (не экспортируются)
// ─────────────────────────────────────────────────────────────

/// Фоновая задача: транслирует строки из mpsc-канала в WebSocket.
///
/// # Почему вынесено в отдельную функцию?
///
/// - Упрощает `split_socket`: одна ответственность — «разделить и вернуть»
/// - Позволяет протестировать логику отправки изолированно (в будущем)
/// - Делает код читаемым: название функции объясняет, что делает блок
///
/// # Завершение
///
/// Задача завершается, если:
/// - `rx.recv()` вернёт `None` (канал закрыт с нашей стороны)
/// - `ws_sender.send()` вернёт ошибку (соединение разорвано)
fn spawn_sender_task(
    mut ws_sender: futures_util::stream::SplitSink<WebSocket, Message>,
    mut rx: mpsc::UnboundedReceiver<String>,
) {
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = ws_sender.send(Message::Text(msg)).await {
                // Ошибка отправки = разрыв соединения. Логируем на уровне debug,
                // чтобы не спамить в продакшене при штатных отключениях.
                debug!("WebSocket send failed (peer disconnected): {}", e);
                break;
            }
        }
        debug!("WebSocket sender task finished");
    });
}

// ─────────────────────────────────────────────────────────────
// Вспомогательные функции (опционально, для удобства)
// ─────────────────────────────────────────────────────────────

/// Извлекает текст из сообщения WebSocket, если оно текстовое.
///
/// # Возвращает
///
/// - `Some(String)` — если сообщение `Message::Text`
/// - `None` — для бинарных данных, пингов, закрытий
///
/// # Пример
///
/// ```rust
/// # use axum::extract::ws::Message;
/// # use signaling_server::transport::try_extract_text;
/// let msg = Message::Text("hello".into());
/// assert_eq!(try_extract_text(&msg), Some("hello".to_string()));
/// ```
#[inline]
pub fn try_extract_text(msg: &Message) -> Option<String> {
    match msg {
        Message::Text(s) => Some(s.clone()),
        _ => None,
    }
}

/// Логирует тип входящего сообщения для отладки.
///
/// Используйте в режиме `debug` для анализа потока сообщений.
/// В продакшене отключается через уровень логирования `RUST_LOG`.
#[inline]
pub fn debug_log_message(msg: &Message) {
    match msg {
        Message::Text(s) => debug!("← Text ({} bytes)", s.len()),
        Message::Binary(b) => debug!("← Binary ({} bytes)", b.len()),
        Message::Ping(_) => debug!("← Ping"),
        Message::Pong(_) => debug!("← Pong"),
        Message::Close(f) => debug!("← Close: {:?}", f),
    }
}
