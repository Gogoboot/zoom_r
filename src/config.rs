//! Конфигурация сервера.
//!
//! Загружает и валидирует настройки из переменных окружения.
//! Использует типобезопасный `enum` для уровня логирования.
//!
//! # Переменные окружения
//!
//! | Переменная | Описание | По умолчанию |
//! |------------|----------|--------------|
//! | `PORT` | Порт сервера | `3000` |
//! | `LOG_LEVEL` | Уровень логирования | `debug` |

use crate::error::{AppError, AppResult};
use serde::Deserialize;
use std::str::FromStr;

/// Уровень логирования.
///
/// Реализует `FromStr` для ручного парсинга и `Deserialize` для `envy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Default for LogLevel {
    fn default() -> Self { Self::Debug }
}

/// ✅ Реализация трейта FromStr (исправляет вашу ошибку)
impl FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "error" => Ok(Self::Error),
            "warn"  => Ok(Self::Warn),
            "info"  => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            "trace" => Ok(Self::Trace),
            _ => Err(format!(
                "Unknown log level: '{}'. Valid: error, warn, info, debug, trace", s
            )),
        }
    }
}

impl LogLevel {
    /// Возвращает строковое представление для `tracing-subscriber`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn  => "warn",
            Self::Info  => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

/// Конфигурация сервера.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default)]
    pub log_level: LogLevel,
}

/// Дефолтное значение для `port`.
#[inline]
fn default_port() -> u16 { 3000 }

impl Config {
    /// Загружает конфигурацию из переменных окружения.
    pub fn from_env() -> AppResult<Self> {
        dotenvy::dotenv().ok();

        let config = envy::from_env::<Config>()
            .map_err(|e| AppError::Config(format!("Failed to parse config: {}", e)))?;

        // Валидация: порт 0 зарезервирован ОС для автовыбора
        if config.port == 0 {
            return Err(AppError::Config(
                "PORT cannot be 0. Use 1-65535.".to_string()
            ));
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_parsing() {
        assert_eq!(LogLevel::from_str("DEBUG").unwrap(), LogLevel::Debug);
        assert_eq!(LogLevel::from_str("info").unwrap(), LogLevel::Info);
        assert!(LogLevel::from_str("invalid").is_err());
    }

    #[test]
    fn test_log_level_as_str() {
        assert_eq!(LogLevel::Debug.as_str(), "debug");
    }

    #[test]
    fn test_port_zero_validation() {
        std::env::set_var("PORT", "0");
        std::env::remove_var("LOG_LEVEL");
        
        let result = Config::from_env();
        assert!(result.is_err());
        
        std::env::remove_var("PORT");
    }
}
