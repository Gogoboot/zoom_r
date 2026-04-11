// // axum::extract::WebSocketUpgrade — тип, 
// // который помогает "поднять" HTTP-запрос до WebSocket.
// // response::Response — HTTP-ответ.
// // routing::get — обработчик GET-запросов.
// // Router — главный маршрутизатор Axum.
// // std::net::SocketAddr — структура для хранения IP-адреса и порта.
// // tracing::info — макрос для логирования информационных сообщений.

// // Что происходит во время работы
// // 1. Вы запускаете cargo run. Компилируется программа и запускается.
// // 2. Сервер начинает слушать порт 3000 (или другой).
// // 3. Клиент (браузер) открывает WebSocket соединение на ws://localhost:3000/ws.
// // 4. Axum вызывает websocket_handler, передавая туда запрос на обновление.
// // 5. Если обновление успешно, выполняется замыкание внутри on_upgrade.
// // 6. Сокет разделяется на sender и receiver.
// // 7. Цикл читает сообщения от клиента и отправляет их обратно (эхо).
// use tokio::net::TcpListener;

// use axum::{
//     extract::WebSocketUpgrade,
//     response::Response,
//     routing::get,
//     Router,
// };
// use std::net::SocketAddr;
// use tracing::info;

// #[tokio::main]
// async fn main() -> anyhow::Result<()> {
//     // Загрузка переменных окружения из .env (если есть)
//     dotenvy::dotenv().ok();
//     // Инициализация логирования
//     tracing_subscriber::fmt()
//         .with_env_filter("signaling_server=debug,tower_http=debug")
//         .init();
//     // Читает переменную окружения PORT. Если её нет, использует "3000". 
//     // Превращает строку в число u16 (порт). ? — если ошибка,
//     // то выходит из функции с ошибкой.
//     let port = std::env::var("PORT")
//         .unwrap_or_else(|_| "3000".to_string())
//         .parse::<u16>()?;

//     //  Создаёт маршрутизатор:
//     //  /ws — обрабатывается функцией websocket_handler при GET-запросе.
//     //  /health — возвращает строку "ok" (проверка работоспособности).
//     let app = Router::new()
//         .route("/ws", get(websocket_handler))
//         .route("/health", get(|| async { "ok" }));


//     // Адрес 0.0.0.0:port — слушаем все сетевые интерфейсы.
//     let addr = SocketAddr::from(([0, 0, 0, 0], port));

//     // Запускаем сервер. await — ждём, пока сервер не остановится. ? — пробрасываем ошибку.
//     info!("Signaling server listening on {}", addr);
//     axum::Server::bind(&addr)
//         .serve(app.into_make_service())
//         .await?;

//     Ok(())
// }
// // ws.on_upgrade принимает замыкание (анонимную функцию), 
// // которое будет вызвано, когда соединение успешно "обновлено"
// // до WebSocket. Это замыкание получает объект socket типа WebSocket.
// async fn websocket_handler(ws: WebSocketUpgrade) -> Response {
//     ws.on_upgrade(|socket| async {
//         // Обработка WebSocket соединения (эхо-функция)
//         // split() разделяет сокет на две половины: 
//         // sender (отправитель) и receiver (получатель). 
//         // Это нужно, чтобы одновременно читать и писать.
//         use futures_util::{SinkExt, StreamExt};
//         let (mut sender, mut receiver) = socket.split();

//         // Цикл, который получает следующее сообщение из receiver. 
//         // next().await ждёт, пока прийдёт сообщение. 
//         // Сообщение обёрнуто в Option (если None — соединение закрыто)
//         // и в Result (ошибка). 
//         // Some(Ok(msg)) — значит сообщение успешно получено.
//         while let Some(Ok(msg)) = receiver.next().await {
            
//             // Если сообщение — текстовое (не двоичное), 
//             //то отправляем его обратно (эхо).
//             // _ = sender.send(...) игнорирует возможную ошибку отправки 
//             // (хотя в реальном проекте надо бы обрабатывать).
//             if let axum::extract::ws::Message::Text(text) = msg {
//                 let _ = sender.send(axum::extract::ws::Message::Text(text)).await;
//             }
//         }
//     })
// }

//! Signaling сервер для WebRTC.
//!
//! Этот бинарный крейт реализует signalling-сервер для обмена SDP и ICE кандидатами
//! между клиентами WebRTC. Сервер управляет комнатами, участниками и ретранслирует
//! сообщения (offer, answer, ice) между пирами.
//!
//! # Архитектура
//!
//! - WebSocket сервер на Axum.
//! - Хранение комнат в памяти с помощью `DashMap` (потокобезопасно).
//! - Для каждого WebSocket соединения создаётся отдельная задача, использующая канал
//!   для отправки сообщений.
//! - При разрыве соединения участник автоматически удаляется из комнаты.
//!
//! # Запуск
//!
//! ```bash
//! cargo run
//! # или с переменной окружения PORT
//! PORT=8080 cargo run
//! ```
//!
//! # Протокол
//!
//! Клиент должен отправить первое сообщение `CreateRoom` или `JoinRoom`.
//! Далее обмен `Offer`/`Answer`/`IceCandidate` происходит через сервер.
//! Подробности см. в структурах `ClientMessage` и `ServerMessage`.

//! Сервер сигналинга для WebRTC
//!
//! Этот крейт реализует signalling-сервер для обмена SDP и ICE кандидатами
//! между клиентами WebRTC. Поддерживает создание комнат, управление участниками
//! и ретрансляцию сообщений.

use axum::{
    extract::{ws::WebSocket, WebSocketUpgrade, State},
    response::Response,
    routing::get,
    Router,
};
use axum::extract::ws::Message;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::mpsc;
use tokio::net::TcpListener;
use tracing::{info, debug}; // warn, error пока не используются

/// Тип сообщения, которое клиент отправляет серверу
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    /// Создать новую комнату (сервер сгенерирует room_id)
    CreateRoom,
    /// Присоединиться к существующей комнате
    JoinRoom {
        room_id: String,
        display_name: Option<String>,
    },
    /// Покинуть комнату
    LeaveRoom,
    /// SDP offer для установки соединения с целевым участником
    Offer {
        target_id: String,
        sdp: String,
    },
    /// SDP answer
    Answer {
        target_id: String,
        sdp: String,
    },
    /// ICE кандидат
    IceCandidate {
        target_id: String,
        candidate: String,
    },
}

/// Тип сообщения, которое сервер отправляет клиенту
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ServerMessage {
    /// Ответ на `CreateRoom` – комната создана
    RoomCreated {
        room_id: String,
        participant_id: String,
    },
    /// Ответ на `JoinRoom` – комната присоединена, список участников
    RoomJoined {
        room_id: String,
        participants: Vec<ParticipantInfo>,
    },
    /// Уведомление о том, что новый участник вошёл в комнату
    ParticipantJoined {
        participant: ParticipantInfo,
    },
    /// Уведомление о том, что участник покинул комнату
    ParticipantLeft {
        participant_id: String,
    },
    /// SDP offer от другого участника
    Offer {
        from_id: String,
        sdp: String,
    },
    /// SDP answer от другого участника
    Answer {
        from_id: String,
        sdp: String,
    },
    /// ICE кандидат от другого участника
    IceCandidate {
        from_id: String,
        candidate: String,
    },
    /// Ошибка (например, комната не найдена)
    Error {
        message: String,
    },
}

/// Информация об участнике для отправки другим клиентам
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ParticipantInfo {
    id: String,
    display_name: String,
}

// ---- Внутренние структуры для хранения состояния ----

/// Участник внутри сервера
#[derive(Debug)]
struct Participant {
    id: String,
    display_name: String,
    /// Канал для отправки сообщений в WebSocket этого участника
    sender: mpsc::UnboundedSender<String>,
}

/// Комната – контейнер участников
#[allow(dead_code)] // поле id пока не используется, но нужно для будущих логов
#[derive(Debug)]
struct Room {
    id: String,
    participants: HashMap<String, Participant>,
}

impl Room {
    fn new(id: String) -> Self {
        Self {
            id,
            participants: HashMap::new(),
        }
    }

    fn add_participant(&mut self, participant: Participant) {
        self.participants.insert(participant.id.clone(), participant);
    }

    fn remove_participant(&mut self, participant_id: &str) -> Option<Participant> {
        self.participants.remove(participant_id)
    }
}

/// Тип для потокобезопасного менеджера комнат
type RoomManager = Arc<DashMap<String, Room>>;

/// Контекст, связанный с конкретным WebSocket соединением
struct ConnectionContext {
    participant_id: String,
    room_id: String,
    manager: RoomManager,
}

// ---- Основная логика сервера ----

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter("signaling_server=debug,tower_http=debug")
        .init();

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()?;

    let room_manager: RoomManager = Arc::new(DashMap::new());

    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .route("/health", get(|| async { "ok" }))
        .with_state(room_manager);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;
    info!("Signaling server listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}

/// Обработчик WebSocket upgrade
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(room_manager): State<RoomManager>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, room_manager))
}

/// Обрабатывает активное WebSocket соединение
async fn handle_socket(socket: WebSocket, room_manager: RoomManager) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    let mut ctx: Option<ConnectionContext> = None;

    while let Some(Ok(msg)) = ws_receiver.next().await {
        match msg {
            Message::Text(text) => {
                debug!("Received: {}", text);
                let result = if let Some(ctx) = &ctx {
                    handle_message(ctx, &text).await
                } else {
                    handle_initial_message(&room_manager, &text, tx.clone()).await
                };
                match result {
                    Ok(new_ctx) => {
                        if let Some(new_ctx) = new_ctx {
                            ctx = Some(new_ctx);
                        }
                    }
                    Err(err_msg) => {
                        let _ = tx.send(
                            serde_json::to_string(&ServerMessage::Error { message: err_msg }).unwrap(),
                        );
                        break;
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    // При закрытии соединения удаляем участника из комнаты
    if let Some(ctx) = ctx {
        let room_id = ctx.room_id.clone();
        let participant_id = ctx.participant_id.clone();
        if let Some(mut room) = room_manager.get_mut(&room_id) {
            room.remove_participant(&participant_id);
            let left_msg = ServerMessage::ParticipantLeft { participant_id: participant_id.clone() };
            let left_text = serde_json::to_string(&left_msg).unwrap();
            for (_, p) in room.participants.iter() {
                let _ = p.sender.send(left_text.clone());
            }
            if room.participants.is_empty() {
                drop(room);
                room_manager.remove(&room_id);
                debug!("Room {} deleted (empty)", room_id);
            }
        }
    }

    send_task.abort();
}

/// Обработка первого сообщения от клиента (создание или вход в комнату)
async fn handle_initial_message(
    room_manager: &RoomManager,
    text: &str,
    sender: mpsc::UnboundedSender<String>,
) -> Result<Option<ConnectionContext>, String> {
    let msg: ClientMessage = serde_json::from_str(text).map_err(|e| format!("Invalid JSON: {}", e))?;
    match msg {
        ClientMessage::CreateRoom => {
            let room_id = uuid::Uuid::new_v4().to_string();
            let participant_id = uuid::Uuid::new_v4().to_string();
            let display_name = format!("User_{}", &participant_id[0..5]);
            let participant = Participant {
                id: participant_id.clone(),
                display_name: display_name.clone(),
                sender: sender.clone(),
            };
            let mut room = Room::new(room_id.clone());
            room.add_participant(participant);
            room_manager.insert(room_id.clone(), room);
            let response = ServerMessage::RoomCreated {
                room_id: room_id.clone(),
                participant_id: participant_id.clone(),
            };
            sender.send(serde_json::to_string(&response).unwrap()).map_err(|_| "Cannot send")?;
            Ok(Some(ConnectionContext {
                participant_id,
                room_id,
                manager: room_manager.clone(),
            }))
        }
        ClientMessage::JoinRoom { room_id, display_name } => {
            let mut room_entry = room_manager.get_mut(&room_id).ok_or("Room not found")?;
            let participant_id = uuid::Uuid::new_v4().to_string();
            let display_name = display_name.unwrap_or_else(|| format!("User_{}", &participant_id[0..5]));
            let participant = Participant {
                id: participant_id.clone(),
                display_name: display_name.clone(),
                sender: sender.clone(),
            };
            let joined_msg = ServerMessage::ParticipantJoined {
                participant: ParticipantInfo {
                    id: participant_id.clone(),
                    display_name: display_name.clone(),
                },
            };
            let joined_text = serde_json::to_string(&joined_msg).unwrap();
            for (_, p) in room_entry.participants.iter() {
                let _ = p.sender.send(joined_text.clone());
            }
            room_entry.add_participant(participant);
            let participants_list: Vec<ParticipantInfo> = room_entry
                .participants
                .iter()
                .map(|(id, p)| ParticipantInfo {
                    id: id.clone(),
                    display_name: p.display_name.clone(),
                })
                .collect();
            let room_joined_msg = ServerMessage::RoomJoined {
                room_id: room_id.clone(),
                participants: participants_list,
            };
            sender.send(serde_json::to_string(&room_joined_msg).unwrap()).map_err(|_| "Cannot send")?;
            Ok(Some(ConnectionContext {
                participant_id,
                room_id,
                manager: room_manager.clone(),
            }))
        }
        _ => Err("First message must be CreateRoom or JoinRoom".to_string()),
    }
}

/// Обработка последующих сообщений (offer, answer, ice, leave)
async fn handle_message(ctx: &ConnectionContext, text: &str) -> Result<Option<ConnectionContext>, String> {
    let msg: ClientMessage = serde_json::from_str(text).map_err(|e| format!("Invalid JSON: {}", e))?;
    let room_id = ctx.room_id.clone();
    let sender_id = ctx.participant_id.clone();
    let room_guard = ctx.manager.get(&room_id).ok_or("Room not found")?;
    match msg {
        ClientMessage::Offer { target_id, sdp } => {
            let target = room_guard.participants.get(&target_id).ok_or("Target not found")?;
            let offer_msg = ServerMessage::Offer { from_id: sender_id, sdp };
            let offer_text = serde_json::to_string(&offer_msg).unwrap();
            target.sender.send(offer_text).map_err(|_| "Target disconnected")?;
        }
        ClientMessage::Answer { target_id, sdp } => {
            let target = room_guard.participants.get(&target_id).ok_or("Target not found")?;
            let answer_msg = ServerMessage::Answer { from_id: sender_id, sdp };
            let answer_text = serde_json::to_string(&answer_msg).unwrap();
            target.sender.send(answer_text).map_err(|_| "Target disconnected")?;
        }
        ClientMessage::IceCandidate { target_id, candidate } => {
            let target = room_guard.participants.get(&target_id).ok_or("Target not found")?;
            let ice_msg = ServerMessage::IceCandidate { from_id: sender_id, candidate };
            let ice_text = serde_json::to_string(&ice_msg).unwrap();
            target.sender.send(ice_text).map_err(|_| "Target disconnected")?;
        }
        ClientMessage::LeaveRoom => {
            return Err("Leave requested".to_string());
        }
        _ => return Err("Unexpected message".to_string()),
    }
    Ok(None)
}
