//! Сущность участника комнаты.
//!
//! [`Participant`] представляет подключённого клиента в контексте конкретной комнаты.
//! Содержит идентификаторы, отображаемое имя и канал для отправки сообщений.

use tokio::sync::mpsc::UnboundedSender;

/// Участник WebRTC-комнаты.
///
/// # Поля
///
/// - `id`: уникальный идентификатор участника в рамках комнаты (обычно UUID)
/// - `display_name`: человекочитаемое имя для отображения в интерфейсе
/// - `sender`: канал для отправки сообщений этому участнику через WebSocket
///
/// # Потокобезопасность
///
/// [`UnboundedSender`] реализует [`Clone`] и может безопасно передаваться
/// между асинхронными задачами. Клонирование увеличивает счётчик ссылок
/// и не копирует внутренние данные — операция дешёвая.
///
/// # Пример
///
/// ```rust
/// # use tokio::sync::mpsc::unbounded_channel;
/// # use signaling_server::domain::Participant;
/// let (tx, _rx) = unbounded_channel::<String>();
/// let participant = Participant::new(
///     "user-123".into(),
///     "Alice".into(),
///     tx,
/// );
/// assert_eq!(participant.id, "user-123");
/// ```
#[derive(Debug, Clone)]  // ✅ Clone важен: UnboundedSender поддерживает клонирование
pub struct Participant {
    /// Уникальный идентификатор участника.
    pub id: String,

    /// Отображаемое имя участника.
    ///
    /// Может быть пустым или содержать значение по умолчанию ("Anonymous"),
    /// если клиент не указал имя при присоединении.
    pub display_name: String,

    /// Канал для отправки сообщений участнику.
    ///
    /// # Важное поведение
    ///
    /// Метод [`send`](UnboundedSender::send) возвращает ошибку, если:
    /// - Получатель (`UnboundedReceiver`) был закрыт
    /// - Участник отключился (разрыв WebSocket)
    ///
    /// В этом случае участник должен быть удалён из комнаты.
    /// Обработку ошибки выполняет вызывающая сторона (обычно оркестратор).
    ///
    /// # Пример отправки
    ///
    /// ```rust
    /// # use signaling_server::domain::Participant;
    /// # fn example(p: &Participant) {
    /// if let Err(e) = p.sender.send("hello".to_string()) {
    ///     eprintln!("Failed to send: {}", e); // участник отключился
    /// }
    /// # }
    /// ```
    #[doc(hidden)]  // 🎯 Скрываем из публичной документации, т.к. это деталь реализации
    pub sender: UnboundedSender<String>,
}

impl Participant {
    /// Создаёт нового участника с заданными параметрами.
    ///
    /// # Аргументы
    ///
    /// - `id`: уникальный идентификатор участника
    /// - `display_name`: отображаемое имя (может быть "Anonymous")
    /// - `sender`: канал для отправки сообщений этому участнику
    ///
    /// # Возвращает
    ///
    /// Новый экземпляр [`Participant`], готовый к добавлению в комнату.
    ///
    /// # Пример
    ///
    /// ```rust
    /// # use tokio::sync::mpsc::unbounded_channel;
    /// # use signaling_server::domain::Participant;
    /// let (tx, _rx) = unbounded_channel::<String>();
    /// let p = Participant::new("u1".into(), "Bob".into(), tx);
    /// ```
    pub fn new(id: String, display_name: String, sender: UnboundedSender<String>) -> Self {
        Self { id, display_name, sender }
    }

    /// Возвращает публичную информацию об участнике.
    ///
    /// Этот метод используется для создания [`crate::domain::ParticipantInfo`],
    /// который передаётся другим клиентам (без внутренних деталей, таких как `sender`).
    ///
    /// # Возвращает
    ///
    /// Структура [`crate::domain::ParticipantInfo`] с `id` и `display_name`.
    ///
    /// # Пример
    ///
    /// ```rust
    /// # use tokio::sync::mpsc::unbounded_channel;
    /// # use signaling_server::domain::Participant;
    /// # let (tx, _rx) = unbounded_channel::<String>();
    /// let p = Participant::new("u1".into(), "Alice".into(), tx);
    /// let info = p.to_info();
    /// assert_eq!(info.id, "u1");
    /// assert_eq!(info.display_name, "Alice");
    /// ```
    #[inline]
    pub fn to_info(&self) -> crate::domain::ParticipantInfo {
        crate::domain::ParticipantInfo {
            id: self.id.clone(),
            display_name: self.display_name.clone(),
        }
    }

    /// Проверяет, можно ли отправить сообщение этому участнику.
    ///
    /// # Возвращает
    ///
    /// - `true` — если получатель канала ещё не закрыт (участник, вероятно, онлайн)
    /// - `false` — если получатель закрыт (участник отключился, но ещё не удалён)
    ///
    /// # Примечание
    ///
    /// Это «мягкая» проверка: между вызовом `is_connected()` и фактической отправкой
    /// участник может отключиться. Всегда обрабатывайте ошибку [`send`](UnboundedSender::send).
    #[inline]
    pub fn is_connected(&self) -> bool {
        !self.sender.is_closed()
    }
}
