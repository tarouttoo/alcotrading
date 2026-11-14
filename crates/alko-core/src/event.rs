//! Нормализованные события единого формата (Q31): без JSON конкретной биржи.
//! Нормализация сырых кадров — задача коннекторов; ядро и все потребители
//! (запись, стратегия, replay) оперируют только этими типами.

use crate::book::Level;
use crate::pair::Pair;
use crate::seq::BookSeq;
use crate::ts::Timestamps;

/// Заголовок события: пара, четыре метки времени PERD (ADR 0005) и последняя
/// измеренная поправка часов биржи.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EventHeader {
    /// Канонический идентификатор пары (`BASEUSDT`, ADR 0004).
    pub pair: Pair,
    /// Метки времени PERD; `delivered` проставляет очередь потребителя.
    pub ts: Timestamps,
    /// `exchangeClockOffsetUs`: биржевые часы − локальные, микросекунды;
    /// последнее измеренное значение (REST server time раз в 60 с, ADR 0005).
    pub clock_offset_us: i64,
}

/// Нормализованное событие единого формата: одна запись конвейера (строка
/// Parquet, аргумент стратегии).
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub header: EventHeader,
    pub kind: EventKind,
}

/// Виды событий (спека #13): обновление книги, сделка, разрыв с границами,
/// плановый разрыв, пропуск сделок.
#[derive(Clone, Debug, PartialEq)]
pub enum EventKind {
    /// Обновление книги: применённый снимок или дельта с итоговыми уровнями
    /// и флагом валидности.
    BookUpdate(BookUpdate),
    /// Сырая сделка с нативным идентификатором биржи (ADR 0006).
    Trade(Trade),
    /// Разрыв потока книги с границами «от последнего валидного обновления
    /// до первого после восстановления» (снимка, вернувшего валидность).
    Gap(Gap),
    /// Плановый разрыв (например, плановое переподключение Binance на
    /// 24-часовой отметке). Книга невалидна до следующего снимка; отдельный
    /// [`EventKind::Gap`] при восстановлении не отдаётся — разрыв уже помечен.
    PlannedGap,
    /// Пропуск сделок: разрыв в ленте сделок, обнаруженный по монотонности
    /// нативных идентификаторов; пишется как событие и не инвалидирует
    /// книгу (глоссарий, ADR 0006).
    TradeGap(TradeGap),
}

/// Применённое обновление книги.
#[derive(Clone, Debug, PartialEq)]
pub struct BookUpdate {
    /// Заявки (bids), по убыванию цены, топ-20.
    pub bids: Vec<Level>,
    /// Запросы (asks), по возрастанию цены, топ-20.
    pub asks: Vec<Level>,
    /// Валидность книги: `false` после разрыва или переподключения, до
    /// следующего снимка от биржи; стратегия и replay обязаны это видеть
    /// (спека #13). Содержимое при `false` — устаревшее (последнее валидное).
    pub valid: bool,
    /// Метка последовательности применённого состояния книги; у маркера
    /// невалидности — метка последнего валидного состояния.
    pub seq: BookSeq,
}

/// Сырая сделка: как отдал матчинг биржи (глоссарий), без агрегирования.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trade {
    /// Нативный идентификатор сделки (`t` Binance / `seq` Bybit /
    /// `tradeId` OKX, ADR 0006).
    pub trade_id: i64,
    pub price: f64,
    pub qty: f64,
    /// Сторона тейкера (агрессора).
    pub taker_side: TradeSide,
}

/// Сторона тейкера сделки.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradeSide {
    /// Агрессор покупал (сделка по цене запроса).
    Buy,
    /// Агрессор продавал (сделка по цене заявки).
    Sell,
}

/// Границы разрыва потока книги (спека #13): ничего не додумывается —
/// только фактические идентификаторы вокруг дыры.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gap {
    /// Идентификатор последнего валидного обновления до разрыва.
    pub last_valid_update_id: i64,
    /// Идентификатор первого обновления после восстановления (снимка,
    /// вернувшего валидность книге).
    pub first_recovered_update_id: i64,
}

/// Пропуск сделок: непрерывность нативных идентификаторов нарушена.
/// Книгу не инвалидирует (ADR 0006).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TradeGap {
    /// Идентификатор последней сделки до разрыва.
    pub last_trade_id: i64,
    /// Идентификатор первой сделки после разрыва.
    pub first_trade_id: i64,
    /// Сколько сделок пропущено: `first − last − 1`.
    pub skipped: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Заголовок события несёт пару, метки PERD и поправку часов биржи
    /// (ADR 0005); поправка знаковая — часы биржи могут отставать.
    #[test]
    fn header_carries_pair_timestamps_and_clock_offset() {
        let header = EventHeader {
            pair: Pair::BTCUSDT,
            ts: Timestamps {
                event: Some(1_700_000_000_000_000),
                push: None,
                received: Some(1_700_000_000_000_500),
                delivered: None,
            },
            clock_offset_us: -200,
        };
        let event = Event {
            header,
            kind: EventKind::PlannedGap,
        };
        assert_eq!(event.header.pair, Pair::BTCUSDT);
        assert_eq!(event.header.ts.push, None);
        assert_eq!(event.header.clock_offset_us, -200);
    }

    /// Границы разрыва и пропуска сделок — фактические идентификаторы
    /// вокруг дыры; количество пропущенных сделок выводится из границ.
    #[test]
    fn gap_boundaries_bracket_the_hole() {
        let gap = Gap {
            last_valid_update_id: 100,
            first_recovered_update_id: 205,
        };
        assert_eq!(
            gap.first_recovered_update_id - gap.last_valid_update_id,
            105
        );

        let trade_gap = TradeGap {
            last_trade_id: 10,
            first_trade_id: 13,
            skipped: 2,
        };
        assert_eq!(
            trade_gap.skipped,
            trade_gap.first_trade_id - trade_gap.last_trade_id - 1
        );
    }
}
