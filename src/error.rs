//! Глобальный тип ошибки приложения.
//!
//! Этот модуль определяет [`AppError`] — единый тип ошибки, который объединяет
//! все возможные ошибки подсистем: конфигурации, домена, обработчиков, инфраструктуры
//! и транспорта. Это позволяет использовать оператор `?` для автоматического
//! преобразования ошибок на всех уровнях.
//!
//! # Пример использования
//!
//! ```rust
//! use signaling_server::{AppError, AppResult};
//!
//! fn do_something() -> AppResult<String> {
//!     // Любая ошибка, реализующая Into<AppError>, автоматически конвертируется
//!     let data = std::fs::read_to_string("config.json")?; // io::Error → AppError::Io
//!     Ok(data)
//! }
//! ```

use thiserror::Error;

/// Глобальная ошибка приложения.
///
/// Объединяет ошибки всех слоёв архитектуры. Каждый вариант соответствует
/// определённому модулю и содержит контекстную информацию об ошибке.
///
/// # Паттерн использования
///
/// - В `domain/` возвращайте `DomainError`
/// - В `handlers/` возвращайте `HandlerError`
/// - В `main.rs` и публичном API используйте `AppResult<T>`
///
/// Все ошибки автоматически конвертируются в `AppError` благодаря `#[from]`.
#[derive(Debug, Error)]
pub enum AppError {
    /// Ошибка конфигурации: невалидное значение, отсутствие переменной и т.д.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Ошибка ввода-вывода: проблемы с файлами, сетью, сокетами.
    ///
    /// Автоматическая конвертация: `std::io::Error` → `AppError::Io`
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Ошибка сериализации/десериализации JSON.
    ///
    /// Автоматическая конвертация: `serde_json::Error` → `AppError::Json`
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// Ошибка бизнес-логики (доменный слой).
    ///
    /// Автоматическая конвертация: `domain::DomainError` → `AppError::Domain`
    #[error("Domain logic error: {0}")]
    Domain(#[from] DomainError),

    /// Ошибка обработки команды (слой handlers).
    ///
    /// Автоматическая конвертация: `handlers::HandlerError` → `AppError::Handler`
    #[error("Handler error: {0}")]
    Handler(#[from] HandlerError),

    /// Ошибка инфраструктуры: хранилище, БД, внешние сервисы.
    ///
    /// Автоматическая конвертация: `infrastructure::InfraError` → `AppError::Infrastructure`
    #[error("Infrastructure error: {0}")]
    Infrastructure(#[from] InfraError),

    /// Ошибка транспорта: WebSocket, HTTP, сетевые проблемы.
    ///
    /// Автоматическая конвертация: `transport::TransportError` → `AppError::Transport`
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    /// Ошибка оркестратора: управление состоянием соединения.
    ///
    /// Автоматическая конвертация: `orchestrator::OrchestratorError` → `AppError::Orchestrator`
    #[error("Orchestrator error: {0}")]
    Orchestrator(#[from] OrchestratorError),
}

/// Тип-алиас для `Result` с глобальной ошибкой приложения.
///
/// Используется во всех публичных функциях, которые могут завершиться с ошибкой.
/// Делает сигнатуры короче и единообразными.
///
/// # Пример
///
/// ```rust
/// use signaling_server::AppResult;
///
/// async fn handle_request() -> AppResult<()> {
///     // ... код, использующий оператор ?
///     Ok(())
/// }
/// ```
pub type AppResult<T> = Result<T, AppError>;

// ─────────────────────────────────────────────────────────────
// Реэкспорты ошибок подмодулей для удобства
// ─────────────────────────────────────────────────────────────
// ▶ Поскольку модули приватные, мы не можем написать `use domain::DomainError`
// ▶ Вместо этого реэкспортируем ошибки через crate::error
// ▶ Это позволяет другим модулям импортировать их как `crate::error::DomainError`
// ─────────────────────────────────────────────────────────────
// Реэкспорты ошибок подмодулей
// ─────────────────────────────────────────────────────────────
pub use crate::domain::DomainError;
pub use crate::handlers::HandlerError;
pub use crate::infrastructure::InfraError;
pub use crate::transport::TransportError;
pub use crate::orchestrator::OrchestratorError;