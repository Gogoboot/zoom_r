//! Signaling сервер для WebRTC.
//! Точка входа в приложение.

use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use tracing::info;

// Импортируем публичные типы из нашей библиотеки
use signaling_server::{Config, ConnectionRegistry, MemoryRoomStore, Orchestrator};

#[tokio::main]
async fn main() -> Result<(), signaling_server::AppError> {
    // 1. Конфигурация
    dotenvy::dotenv().ok();
    let config = Config::from_env()?;
    
    tracing_subscriber::fmt()
        .with_env_filter(format!("signaling_server={}", config.log_level.as_str()))
        .init();

    // 2. Инфраструктура
    // Хранилище данных (комнаты, участники)
    let store = MemoryRoomStore::new();
    
    // Реестр подключений (сокеты)
    // Вот он — Registry!
    let registry = ConnectionRegistry::new(); 

    // 3. Оркестратор (Собираем всё вместе)
    // Передаем И хранилище, И реестр
    let orchestrator = Orchestrator::new(store, registry);

    // 4. Запуск сервера
    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .route("/health", get(|| async { "ok" }))
        .with_state(orchestrator);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port)).await?;
    info!("Server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;
    Ok(())
}

async fn websocket_handler(
    ws: axum::extract::WebSocketUpgrade,
    State(orchestrator): State<Orchestrator<MemoryRoomStore>>,
) -> Response {
    ws.on_upgrade(|socket| async move {
        orchestrator.handle_connection(socket).await;
    })
}
