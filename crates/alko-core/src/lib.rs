//! `alko-core`: нормализованные события единого формата (Q31), книга на 20
//! уровней на сторону и валидация нативных последовательностей бирж (тикет #15).
//!
//! Ядро не знает JSON конкретной биржи: коннекторы (#19/#21/#22) парсят сырые
//! кадры и нормализуют их в [`Event`] с числовыми метками последовательностей
//! ([`BookSeq`]) и абсолютными количествами уровней ([`Level`]). Метки времени
//! — четыре nullable-колонки PERD ([`Timestamps`], ADR 0005), сделки — сырые
//! с нативными идентификаторами (ADR 0006), пары — фиксированная пятёрка
//! ADR 0004 ([`Pair::ALL`]).
//!
//! Целостность ведут два потока одного стека «пара × биржа»:
//!
//! - [`BookFeed`] — снимки и дельты с seq-валидацией (Binance `U`/`u` и
//!   правило `pu` — несовпадение значит ресинк со снимка, не ошибку; Bybit
//!   `u` с полным сбросом при `u == 1`; OKX `seqId`/`prevSeqId` с keep-alive
//!   и сбросом нумерации; checksum не используется). Разрыв помечает книгу
//!   невалидной до следующего снимка; потребители видят флаг валидности в
//!   каждом [`EventKind::BookUpdate`], а при восстановлении получают
//!   [`EventKind::Gap`] с границами «от последнего валидного обновления до
//!   первого после восстановления». Плановые переподключения — отдельное
//!   [`EventKind::PlannedGap`].
//! - [`TradeFeed`] — сырые сделки; пропуск в ленте детектится монотонностью
//!   нативных идентификаторов и пишется событием [`EventKind::TradeGap`],
//!   книгу он не инвалидирует.

pub mod book;
pub mod event;
pub mod feed;
pub mod pair;
pub mod seq;
pub mod ts;

pub use book::{BOOK_DEPTH, Book, Level};
pub use event::{BookUpdate, Event, EventHeader, EventKind, Gap, Trade, TradeGap, TradeSide};
pub use feed::{BookFeed, BookInput, TradeFeed};
pub use pair::Pair;
pub use seq::{BookInputKind, BookSeq, SeqScheme};
pub use ts::Timestamps;
