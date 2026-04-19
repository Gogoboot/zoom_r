//! Signaling сервер для WebRTC.
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use tokio::signal;
use tokio::time::Duration;
use tracing::info;

use signaling_server::{Config, ConnectionRegistry, MemoryRoomStore, Orchestrator};

#[tokio::main]
async fn main() -> Result<(), signaling_server::AppError> {
    // 1. Конфигурация
    dotenvy::dotenv().ok();
    let config = signaling_server::Config::from_env()?;
    tracing_subscriber::fmt()
        .with_env_filter(format!("signaling_server={}", config.log_level.as_str()))
        .init();

    // 2. Инфраструктура
    let store = signaling_server::MemoryRoomStore::new();
    let registry = signaling_server::ConnectionRegistry::new(); 

    // 3. Оркестратор
    let orchestrator = signaling_server::Orchestrator::new(store, registry);
    
    // 🔑 ВАЖНО: Клонируем оркестратор перед тем, как он уйдёт в Router
    // Это позволит нам использовать его для shutdown в main функции
    let orchestrator_for_shutdown = orchestrator.clone();

    // 4. Сборка приложения
    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .route("/health", get(|| async { "ok" }))
        .with_state(orchestrator);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port)).await?;
    tracing::info!("🚀 Server listening on {}", listener.local_addr()?);

    // 5. Логика Graceful Shutdown
    // Создаём канал, чтобы передать сигнал из задачи в будущее shutdown
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel(1);

    // Запускаем задачу, которая слушает Ctrl+C
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("Failed to install CTRL+C handler: {}", e);
        }
        // Посылаем сигнал
        let _ = shutdown_tx.send(());
    });

    // 6. Запуск сервера
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            // Ждём сигнала
            let _ = shutdown_rx.recv().await;
            tracing::info!(" Получен сигнал остановки (Ctrl+C).");

            // Этап 1: Вежливо закрываем все соединения (Код 1001)
            orchestrator_for_shutdown.shutdown_connections(1001, "Server Restarting").await;

            // Этап 2: Ждём 2 секунды, пока клиенты получат Close и обработчики (handle_connection)
            // успеют выполнить cleanup (handle_leave_room)
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            
            tracing::info!("✅ Graceful shutdown completed.");
        })
        .await?;

    tracing::info!("🏁 Сервер полностью остановлен.");
    Ok(())
}
// ✅ ИСПРАВЛЕНО: добавлен конкретный тип хранилища
async fn websocket_handler(
    ws: axum::extract::WebSocketUpgrade,
    State(orchestrator): State<Orchestrator<MemoryRoomStore>>, 
) -> Response {
    ws.on_upgrade(|socket| async move {
        orchestrator.handle_connection(socket).await;
    })
}

