//! `alko-core`: нормализованные события единого формата, книга 20 уровней
//! и валидация по нативным последовательностям бирж (тикет #15).

pub mod book;
pub mod pair;
pub mod ts;

pub use pair::Pair;
pub use ts::Timestamps;
