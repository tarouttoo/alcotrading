//! Доставка нормализованных событий в стратегию: единая точка, где события
//! [`alko_core::Event`] маршрутизируются по обработчикам и служебные поля
//! отсекаются. Этой функцией пользуются и replay, и paper — стратегия
//! получает одинаковую картину в бэктесте и в live.

use crate::Strategy;
use crate::intent::Intent;
use crate::view::{BookView, TradeView};
use alko_core::{Event, EventKind};

/// Доставить одно нормализованное событие в стратегию. Обновление книги и
/// сделка попадают в соответствующие обработчики; события целостности
/// (разрыв, плановый разрыв, пропуск сделок) до стратегии не доходят —
/// невалидность книги видна по флагу в [`BookView::valid`] (разрыв
/// инвалидирует книгу до следующего снимка), пропуск сделок ничего не
/// инвалидирует (ADR 0006). Метки времени не трогаются: `delivered`
/// потребитель проставляет на своей копии события до вызова (ADR 0005).
pub fn feed<S>(strategy: &mut S, event: &Event) -> Vec<Intent>
where
    S: Strategy + ?Sized,
{
    match &event.kind {
        EventKind::BookUpdate(update) => strategy.on_book_update(&BookView {
            pair: event.header.pair,
            ts: event.header.ts,
            bids: &update.bids,
            asks: &update.asks,
            valid: update.valid,
        }),
        EventKind::Trade(trade) => strategy.on_trade(TradeView {
            pair: event.header.pair,
            ts: event.header.ts,
            trade: *trade,
        }),
        EventKind::Gap(_) | EventKind::PlannedGap | EventKind::TradeGap(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cancel, Place, Side};
    use alko_core::{BookSeq, EventHeader, Level, Pair, Timestamps, Trade, TradeSide};

    /// Записывающая стратегия: запоминает сводки доставленных событий и
    /// возвращает заготовленные интенты — проверяем, что они доходят от
    /// обработчика до потребителя без изменений.
    #[derive(Default)]
    struct Recorder {
        books: Vec<(Pair, Timestamps, usize, usize, bool)>,
        trades: Vec<(Pair, Trade)>,
        ticks: Vec<i64>,
    }

    impl Strategy for Recorder {
        fn on_book_update(&mut self, update: &BookView<'_>) -> Vec<Intent> {
            self.books.push((
                update.pair,
                update.ts,
                update.bids.len(),
                update.asks.len(),
                update.valid,
            ));
            vec![Intent::Place(Place {
                pair: update.pair,
                client_id: 1,
                side: Side::Buy,
                price: 99.0,
                qty: 1.0,
            })]
        }

        fn on_trade(&mut self, trade: TradeView) -> Vec<Intent> {
            self.trades.push((trade.pair, trade.trade));
            vec![Intent::Cancel(Cancel {
                pair: trade.pair,
                client_id: 1,
            })]
        }

        fn on_timer(&mut self, now_us: i64) -> Vec<Intent> {
            self.ticks.push(now_us);
            Vec::new()
        }
    }

    fn header() -> EventHeader {
        EventHeader {
            pair: Pair::SOLUSDT,
            ts: Timestamps {
                event: Some(42),
                push: None,
                received: Some(43),
                delivered: Some(44),
            },
            clock_offset_us: -7,
        }
    }

    fn book_event(valid: bool) -> Event {
        Event {
            header: header(),
            kind: EventKind::BookUpdate(alko_core::BookUpdate {
                bids: vec![Level::new(99.0, 1.0)],
                asks: vec![Level::new(100.0, 2.0)],
                valid,
                seq: BookSeq::Bybit { update_id: 10 },
            }),
        }
    }

    fn trade_event(trade_id: i64) -> Event {
        Event {
            header: header(),
            kind: EventKind::Trade(Trade {
                trade_id,
                price: 100.0,
                qty: 0.5,
                taker_side: TradeSide::Sell,
            }),
        }
    }

    fn integrity_event() -> Event {
        Event {
            header: header(),
            kind: EventKind::Gap(alko_core::Gap {
                last_valid_update_id: 10,
                first_recovered_update_id: 20,
            }),
        }
    }

    /// Маршрутизация: книга и сделка доходят до обработчиков с доменными
    /// полями (пара, метки PERD, уровни, валидность), события целостности
    /// (разрыв, плановый разрыв, пропуск сделок) до стратегии не доходят,
    /// интенты проходят от обработчика без изменений.
    #[test]
    fn feed_routes_book_and_trade_and_skips_integrity_events() {
        let mut recorder = Recorder::default();
        let stream = [
            book_event(true),
            integrity_event(),
            Event {
                header: header(),
                kind: EventKind::PlannedGap,
            },
            book_event(false),
            Event {
                header: header(),
                kind: EventKind::TradeGap(alko_core::TradeGap {
                    last_trade_id: 1,
                    first_trade_id: 5,
                    skipped: 3,
                }),
            },
            trade_event(1),
            trade_event(5),
        ];
        let intents: Vec<Vec<Intent>> = stream.iter().map(|e| feed(&mut recorder, e)).collect();

        // дошли только две книги и две сделки
        assert_eq!(recorder.books.len(), 2);
        assert_eq!(recorder.trades.len(), 2);
        // доменные поля книги: пара, метки, уровни, валидность
        assert_eq!(recorder.books[0], (Pair::SOLUSDT, header().ts, 1, 1, true));
        assert!(!recorder.books[1].4);
        // сделки дошли целиком
        assert_eq!(recorder.trades[0].1.trade_id, 1);
        assert_eq!(recorder.trades[1].1.trade_id, 5);
        // интенты прошли без изменений: place на книгу, cancel на сделку
        assert_eq!(intents[0], intents[3]);
        assert!(matches!(intents[0][0], Intent::Place(_)));
        assert_eq!(intents[5], intents[6]);
        assert!(matches!(intents[5][0], Intent::Cancel(_)));
        // события целостности доставляют пустоту
        assert!(intents[1].is_empty() && intents[2].is_empty() && intents[4].is_empty());
    }

    /// Стратегия работает за указателем на трейт (replay и paper держат её
    /// в `Box<dyn Strategy>`): трейт объектно-безопасен.
    #[test]
    fn feed_works_behind_dyn_strategy() {
        let mut recorder = Recorder::default();
        let strategy: &mut dyn Strategy = &mut recorder;
        let intents = feed(strategy, &trade_event(1));
        assert!(matches!(intents[0], Intent::Cancel(_)));
    }
}
