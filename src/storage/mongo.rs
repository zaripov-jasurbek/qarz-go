//! Заглушка для будущей MongoDB-реализации. Когда дойдём до неё, реализуем
//! `Storage` через mongodb crate с теми же коллекциями: users, contacts, rooms,
//! items, debts, blocks, invites, sessions. Идентификаторы (`id: String`) будут
//! ложиться в `_id`.
//!
//! Файл оставлен пустым намеренно — `pub struct MongoStorage` появится здесь,
//! когда подключим mongo crate.
