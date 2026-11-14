//! No-op стратегия-заглушка: логирует доставленные события и тики, ничего
//! не выставляет. Тестовая стратегия для CI и приёмки Q56 (replay-отчёт по
//! заглушке); настоящих стратегий фаза 1 не содержит.

use crate::Strategy;
use crate::intent::Intent;
use crate::view::{BookView, TradeView};

/// Стратегия без действий: каждое событие логируется на уровне `debug`,
/// интенты не возвращаются никогда.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NoOp {
    delivered: u64,
}

impl NoOp {
    /// Пустая стратегия: ничего не видел, ничего не выставил.
    pub fn new() -> Self {
        Self::default()
    }

    /// Сколько событий и тиков таймера доставлено в стратегию: обновления
    /// книги, сделки и тики; события целостности не считаются — до стратегии
    /// они не доходят.
    pub fn delivered(&self) -> u64 {
        self.delivered
    }
}

impl Strategy for NoOp {
    fn on_book_update(&mut self, update: &BookView<'_>) -> Vec<Intent> {
        tracing::debug!(
            pair = %update.pair,
            valid = update.valid,
            bids = update.bids.len(),
            asks = update.asks.len(),
            "обновление книги: no-op не действует"
        );
        self.delivered += 1;
        Vec::new()
    }

    fn on_trade(&mut self, trade: TradeView) -> Vec<Intent> {
        tracing::debug!(
            pair = %trade.pair,
            price = trade.trade.price,
            qty = trade.trade.qty,
            taker_side = ?trade.trade.taker_side,
            "сделка: no-op не действует"
        );
        self.delivered += 1;
        Vec::new()
    }

    fn on_timer(&mut self, now_us: i64) -> Vec<Intent> {
        tracing::debug!(now_us, "тик таймера: no-op не действует");
        self.delivered += 1;
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed;
    use crate::{Level, Pair, Timestamps, Trade};
    use alko_core::{BookSeq, BookUpdate, Event, EventHeader, EventKind, Gap, TradeGap, TradeSide};

    /// Синтетический поток событий одной пары: снимок, сделка, дельта,
    /// плановый разрыв, инвалидация, пропуск сделок, сделка, разрыв с
    /// границами, восстановление снимком.
    fn synthetic_stream() -> Vec<Event> {
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
        let book = |bids: &[f64], asks: &[f64], valid: bool, seq: BookSeq| Event {
            header,
            kind: EventKind::BookUpdate(BookUpdate {
                bids: bids.iter().map(|p| Level::new(*p, 1.0)).collect(),
                asks: asks.iter().map(|p| Level::new(*p, 1.0)).collect(),
                valid,
                seq,
            }),
        };
        let trade = |trade_id: i64| Event {
            header,
            kind: EventKind::Trade(Trade {
                trade_id,
                price: 100.0,
                qty: 0.5,
                taker_side: TradeSide::Buy,
            }),
        };
        vec![
            book(
                &[99.5, 99.0],
                &[100.0, 100.5],
                true,
                BookSeq::Binance {
                    first: 100,
                    last: 100,
                    prev_last: 100,
                },
            ),
            trade(1),
            book(
                &[99.5],
                &[100.0],
                true,
                BookSeq::Binance {
                    first: 101,
                    last: 105,
                    prev_last: 100,
                },
            ),
            Event {
                header,
                kind: EventKind::PlannedGap,
            },
            book(
                &[99.5],
                &[100.0],
                false,
                BookSeq::Binance {
                    first: 105,
                    last: 105,
                    prev_last: 100,
                },
            ),
            Event {
                header,
                kind: EventKind::TradeGap(TradeGap {
                    last_trade_id: 1,
                    first_trade_id: 9,
                    skipped: 7,
                }),
            },
            trade(9),
            Event {
                header,
                kind: EventKind::Gap(Gap {
                    last_valid_update_id: 105,
                    first_recovered_update_id: 500,
                }),
            },
            book(
                &[99.0],
                &[100.5],
                true,
                BookSeq::Binance {
                    first: 500,
                    last: 500,
                    prev_last: 500,
                },
            ),
        ]
    }

    /// No-op стратегия по синтетическому потоку (критерий готовности #17):
    /// интентов нет ни на одном событии; доставлены четыре обновления книги
    /// и две сделки — события целостности до стратегии не доходят.
    #[test]
    fn no_op_over_synthetic_stream_places_nothing() {
        let mut noop = NoOp::default();
        for event in synthetic_stream() {
            let intents = feed(&mut noop, &event);
            assert!(
                intents.is_empty(),
                "no-op не выставляет ордера, получил {intents:?}"
            );
        }
        assert_eq!(noop.delivered(), 6);
        // тик таймера тоже без действий
        assert!(noop.on_timer(1_700_000_000_000_500).is_empty());
        assert_eq!(noop.delivered(), 7);
    }
}
