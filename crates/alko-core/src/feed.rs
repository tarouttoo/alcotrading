//! Потоки одного стека «пара × биржа»: книга с seq-валидацией и лента сырых
//! сделок. На входе — нормализованные обновления от коннектора, на выходе —
//! события единого формата.

use crate::book::{Book, Level};
use crate::event::{BookUpdate, Event, EventHeader, EventKind, Gap, Trade, TradeGap};
use crate::seq::{BookInputKind, BookSeq, SeqScheme, SeqVerdict, check_seq};

/// Входящее обновление книги от коннектора: нормализованные уровни
/// (абсолютные количества), вид обновления и метка последовательности.
#[derive(Clone, Debug, PartialEq)]
pub struct BookInput {
    pub header: EventHeader,
    pub kind: BookInputKind,
    pub seq: BookSeq,
    /// Уровни заявок (bids); количества абсолютные, 0 удаляет уровень.
    pub bids: Vec<Level>,
    /// Уровни запросов (asks); количества абсолютные, 0 удаляет уровень.
    pub asks: Vec<Level>,
}

/// Открытый разрыв: что случилось с непрерывностью и помечен ли разрыв как
/// плановый. Существует ровно до следующего снимка.
#[derive(Clone, Copy, Debug)]
struct PendingGap {
    /// Идентификатор последнего валидного обновления до разрыва (`None`,
    /// если валидного состояния ещё не было — докладывать не о чем).
    last_valid_id: Option<i64>,
    /// Разрыв уже отдан событием «плановый разрыв» — при восстановлении
    /// отдельный [`EventKind::Gap`] не нужен.
    planned: bool,
}

/// Книга одной пары на одной бирже с валидацией нативных последовательностей.
/// Правила: дельты применяются только при непрерывной последовательности;
/// разрыв помечает книгу невалидной до следующего снимка; при восстановлении
/// отдаётся событие «разрыв» с границами (если разрыв не был плановым и
/// было что терять). Плановый разрыв и отсоединение инвалидируют книгу сразу.
pub struct BookFeed {
    book: Book,
    scheme: SeqScheme,
    /// Метка последнего применённого обновления (базовая линия).
    last_seq: Option<BookSeq>,
    pending: Option<PendingGap>,
}

/// Разрыв с границами: от последнего валидного обновления до первого после
/// восстановления (спека #13).
fn gap_event(header: EventHeader, from: i64, recovery_id: i64) -> Event {
    Event {
        header,
        kind: EventKind::Gap(Gap {
            last_valid_update_id: from,
            first_recovered_update_id: recovery_id,
        }),
    }
}

impl BookFeed {
    /// Поток книги по схеме биржи; книга невалидна до первого снимка.
    pub fn new(scheme: SeqScheme) -> Self {
        Self {
            book: Book::new(),
            scheme,
            last_seq: None,
            pending: None,
        }
    }

    /// Текущее состояние книги: уровни и флаг валидности (для мониторинга
    /// и REST-сверки).
    pub fn book(&self) -> &Book {
        &self.book
    }

    /// Валидность книги: стратегия и replay видят её в каждом
    /// [`EventKind::BookUpdate`], мониторинг — здесь.
    pub fn is_valid(&self) -> bool {
        self.book.is_valid()
    }

    /// Применить снимок или дельту и вернуть события (обновление книги,
    /// при восстановлении — ещё и разрыв с границами). Пока книга ждёт
    /// снимок, дельты отбрасываются целиком: содержание устарело.
    pub fn apply(&mut self, input: BookInput) -> Vec<Event> {
        let BookInput {
            header,
            kind,
            seq,
            bids,
            asks,
        } = input;
        let baseline = self.last_seq.as_ref().map(BookSeq::own_id);
        if self.pending.is_some() && kind == BookInputKind::Delta {
            return Vec::new();
        }
        let verdict = check_seq(self.scheme, kind, seq, baseline);
        let mut events = Vec::with_capacity(2);
        match verdict {
            SeqVerdict::Continue | SeqVerdict::ContinueWithGap => {
                let recovering = self.pending.take();
                let recovering = recovering.map(|p| (p.planned, p.last_valid_id));
                // разрыв, открывающийся и закрывающийся одним снимком Binance
                // (пропущенные пуш-потока при валидной книге)
                let atomic_gap =
                    if matches!(verdict, SeqVerdict::ContinueWithGap) && recovering.is_none() {
                        baseline
                    } else {
                        None
                    };
                let changed = match kind {
                    BookInputKind::Snapshot => {
                        self.book.snapshot(bids, asks);
                        true
                    }
                    BookInputKind::Delta => self.book.delta(bids, asks),
                };
                self.last_seq = Some(seq);
                let recovery_id = seq.own_id();
                match recovering {
                    // разрыв с границами «от последнего валидного обновления
                    // до первого после восстановления»
                    Some((false, Some(from))) => {
                        events.push(gap_event(header, from, recovery_id));
                    }
                    // плановый разрыв уже отдан событием PlannedGap — границы
                    // при восстановлении не дублируются; валидного состояния
                    // не было — докладывать не о чем
                    Some(_) => {}
                    None => {
                        if let Some(from) = atomic_gap {
                            events.push(gap_event(header, from, recovery_id));
                        }
                    }
                }
                // снимки отдаются всегда (точки синхронизации на записи),
                // дельты — только если изменили видимые уровни
                if kind == BookInputKind::Snapshot || changed {
                    events.push(Event {
                        header,
                        kind: EventKind::BookUpdate(BookUpdate {
                            bids: self.book.bids().to_vec(),
                            asks: self.book.asks().to_vec(),
                            valid: true,
                            seq,
                        }),
                    });
                }
            }
            SeqVerdict::Gap | SeqVerdict::MalformedSnapshot => {
                if self.pending.is_none() {
                    self.pending = Some(PendingGap {
                        last_valid_id: baseline,
                        planned: false,
                    });
                    self.book.invalidate();
                    if let Some(content_seq) = self.last_seq {
                        events.push(self.invalidation_marker(header, content_seq));
                    }
                }
            }
        }
        events
    }

    /// Отметить плановый разрыв (например, плановое переподключение Binance
    /// на 24-часовой отметке): книга инвалидируется сразу, при восстановлении
    /// отдельный разрыв не докладывается.
    pub fn mark_planned_gap(&mut self, header: EventHeader) -> Vec<Event> {
        let mut events = vec![Event {
            header,
            kind: EventKind::PlannedGap,
        }];
        match &mut self.pending {
            Some(open) => open.planned = true,
            None => {
                self.pending = Some(PendingGap {
                    last_valid_id: self.last_seq.as_ref().map(BookSeq::own_id),
                    planned: true,
                });
                self.book.invalidate();
                if let Some(content_seq) = self.last_seq {
                    events.push(self.invalidation_marker(header, content_seq));
                }
            }
        }
        events
    }

    /// Отметить отсоединение (обрыв потока без объявления): книга
    /// инвалидируется сразу; разрыв с границами докладывается при
    /// восстановлении, первым снимком.
    pub fn mark_disconnected(&mut self, header: EventHeader) -> Vec<Event> {
        if self.pending.is_some() {
            return Vec::new();
        }
        self.pending = Some(PendingGap {
            last_valid_id: self.last_seq.as_ref().map(BookSeq::own_id),
            planned: false,
        });
        self.book.invalidate();
        match self.last_seq {
            Some(content_seq) => vec![self.invalidation_marker(header, content_seq)],
            None => Vec::new(),
        }
    }

    /// Маркер невалидности: содержимое устарело (последнее валидное), метка —
    /// последнего валидного состояния. Отдаётся только когда было что терять.
    fn invalidation_marker(&self, header: EventHeader, content_seq: BookSeq) -> Event {
        Event {
            header,
            kind: EventKind::BookUpdate(BookUpdate {
                bids: self.book.bids().to_vec(),
                asks: self.book.asks().to_vec(),
                valid: false,
                seq: content_seq,
            }),
        }
    }
}

/// Лента сырых сделок одной пары: детектит пропуск по монотонности нативных
/// идентификаторов (ADR 0006). Книгу не инвалидирует — это отдельное событие.
#[derive(Clone, Debug, Default)]
pub struct TradeFeed {
    last_trade_id: Option<i64>,
}

impl TradeFeed {
    /// Лента без базовой линии: первая сделка только ставит её.
    pub fn new() -> Self {
        Self::default()
    }

    /// Принять сырую сделку: возвращает событие сделки и, при разрыве
    /// идентификаторов, событие «пропуск сделок». Идентификатор ≤ предыдущего
    /// трактуется как сброс нумерации (переподключение, повтор): разрыв не
    /// пишется, базовая линия сбрасывается; дубли анализатор отсечёт по id.
    pub fn apply(&mut self, header: EventHeader, trade: Trade) -> Vec<Event> {
        let mut events = Vec::with_capacity(2);
        if let Some(last) = self.last_trade_id
            && trade.trade_id > last
        {
            let diff = trade.trade_id - last;
            if diff > 1 {
                events.push(Event {
                    header,
                    kind: EventKind::TradeGap(TradeGap {
                        last_trade_id: last,
                        first_trade_id: trade.trade_id,
                        skipped: diff - 1,
                    }),
                });
            }
        }
        self.last_trade_id = Some(trade.trade_id);
        events.push(Event {
            header,
            kind: EventKind::Trade(trade),
        });
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::BookUpdate;
    use crate::pair::Pair;
    use crate::ts::Timestamps;
    use BookInputKind::{Delta, Snapshot};

    const BINANCE: SeqScheme = SeqScheme::Binance;
    const BYBIT: SeqScheme = SeqScheme::Bybit;
    const OKX: SeqScheme = SeqScheme::Okx;

    fn header() -> EventHeader {
        EventHeader {
            pair: Pair::BTCUSDT,
            ts: Timestamps::default(),
            clock_offset_us: 0,
        }
    }

    fn input(kind: BookInputKind, seq: BookSeq, bids: &[f64], asks: &[f64]) -> BookInput {
        BookInput {
            header: header(),
            kind,
            seq,
            bids: bids.iter().map(|p| Level::new(*p, 1.0)).collect(),
            asks: asks.iter().map(|p| Level::new(*p, 1.0)).collect(),
        }
    }

    fn b(first: i64, last: i64, prev_last: i64) -> BookSeq {
        BookSeq::Binance {
            first,
            last,
            prev_last,
        }
    }

    fn kinds(events: &[Event]) -> Vec<&'static str> {
        events
            .iter()
            .map(|e| match &e.kind {
                EventKind::BookUpdate(_) => "book",
                EventKind::Trade(_) => "trade",
                EventKind::Gap(_) => "gap",
                EventKind::PlannedGap => "planned_gap",
                EventKind::TradeGap(_) => "trade_gap",
            })
            .collect()
    }

    fn book_updates(events: &[Event]) -> Vec<&BookUpdate> {
        events
            .iter()
            .filter_map(|e| match &e.kind {
                EventKind::BookUpdate(u) => Some(u),
                _ => None,
            })
            .collect()
    }

    fn gaps(events: &[Event]) -> Vec<Gap> {
        events
            .iter()
            .filter_map(|e| match &e.kind {
                EventKind::Gap(g) => Some(*g),
                _ => None,
            })
            .collect()
    }

    // --- общий конвейер: снимок + дельты ---

    #[test]
    fn binance_snapshot_then_delta_chain_stays_valid() {
        let mut feed = BookFeed::new(BINANCE);
        let events = feed.apply(input(Snapshot, b(1000, 1000, 1000), &[100.0], &[101.0]));
        assert_eq!(kinds(&events), ["book"]);
        assert_eq!(
            kinds(&feed.apply(input(Delta, b(1001, 1005, 1000), &[99.5, 100.0], &[101.0],))),
            ["book"]
        );
        assert_eq!(
            kinds(&feed.apply(input(
                Delta,
                b(1006, 1010, 1005),
                &[99.5, 100.0],
                &[101.0, 101.5],
            ))),
            ["book"]
        );
        assert!(feed.is_valid());
        assert_eq!(
            feed.book().bids(),
            [Level::new(100.0, 1.0), Level::new(99.5, 1.0)]
        );
        assert!(
            feed.book()
                .bids()
                .iter()
                .is_sorted_by(|a, b| a.price >= b.price)
        );
    }

    #[test]
    fn identical_snapshot_still_emits_book_update_but_unchanged_delta_does_not() {
        let mut feed = BookFeed::new(BINANCE);
        feed.apply(input(Snapshot, b(1, 1, 1), &[100.0], &[101.0]));
        // дельта, не изменившая видимых уровней, — не событие
        assert!(
            feed.apply(input(Delta, b(2, 2, 1), &[100.0], &[101.0]))
                .is_empty()
        );
        // повторный снимок отдаётся всегда: это точка синхронизации на записи
        let events = feed.apply(input(Snapshot, b(3, 3, 2), &[100.0], &[101.0]));
        assert_eq!(kinds(&events), ["book"]);
        assert!(feed.is_valid());
    }

    // --- Binance: разрыв по `pu`, ресинк со снимка ---

    #[test]
    fn binance_pu_mismatch_invalidates_until_snapshot_then_reports_gap() {
        let mut feed = BookFeed::new(BINANCE);
        feed.apply(input(Snapshot, b(1000, 1000, 1000), &[100.0], &[101.0]));
        feed.apply(input(Delta, b(1001, 1005, 1000), &[100.0], &[101.0]));

        // пропущено событие потока: pu = 1002, базовая линия 1005
        let events = feed.apply(input(Delta, b(1006, 1010, 1002), &[100.0], &[101.0]));
        let updates = book_updates(&events);
        assert_eq!(updates.len(), 1);
        assert!(!updates[0].valid);
        assert!(!feed.is_valid());
        // дельты пока разрыва не применяются и событий не пишут
        assert!(
            feed.apply(input(Delta, b(1011, 1015, 1010), &[100.0], &[101.0]))
                .is_empty()
        );
        assert!(!feed.is_valid());

        // снимок восстанавливает валидность и докладывает разрыв с границами
        let events = feed.apply(input(Snapshot, b(5000, 5000, 5000), &[99.0], &[102.0]));
        assert_eq!(kinds(&events), ["gap", "book"]);
        let gap = gaps(&events)[0];
        assert_eq!(gap.last_valid_update_id, 1005);
        assert_eq!(gap.first_recovered_update_id, 5000);
        let updates = book_updates(&events);
        assert!(updates[0].valid);
        assert_eq!(updates[0].bids, [Level::new(99.0, 1.0)]);
        assert!(feed.is_valid());
    }

    #[test]
    fn binance_snapshot_pu_mismatch_closes_gap_atomically() {
        // канал depth20: каждый пуш — снимок; пропущенные пуши видны по `pu`,
        // но состояние полное — разрыв докладывается и закрывается сразу
        let mut feed = BookFeed::new(BINANCE);
        feed.apply(input(Snapshot, b(100, 100, 100), &[100.0], &[101.0]));
        let events = feed.apply(input(Snapshot, b(500, 500, 405), &[100.0], &[101.0]));
        assert_eq!(kinds(&events), ["gap", "book"]);
        let gap = gaps(&events)[0];
        assert_eq!(gap.last_valid_update_id, 100);
        assert_eq!(gap.first_recovered_update_id, 500);
        assert!(feed.is_valid());
        // цепочка продолжается от нового снимка
        assert_eq!(
            kinds(&feed.apply(input(Delta, b(501, 505, 500), &[99.5, 100.0], &[101.0],))),
            ["book"]
        );
    }

    // --- Bybit: `u` подряд, `u == 1` — полный сброс ---

    #[test]
    fn bybit_full_reset_on_u_one_reports_gap_and_restores_book() {
        let mut feed = BookFeed::new(BYBIT);
        feed.apply(input(
            Snapshot,
            BookSeq::Bybit { update_id: 1 },
            &[100.0],
            &[101.0],
        ));
        feed.apply(input(
            Delta,
            BookSeq::Bybit { update_id: 2 },
            &[100.0],
            &[101.0],
        ));
        feed.apply(input(
            Delta,
            BookSeq::Bybit { update_id: 3 },
            &[99.5],
            &[101.0],
        ));

        // пропуск обновления: ждали 4, пришло 9
        let events = feed.apply(input(
            Delta,
            BookSeq::Bybit { update_id: 9 },
            &[99.5],
            &[101.0],
        ));
        assert_eq!(kinds(&events), ["book"]);
        assert!(!book_updates(&events)[0].valid);
        assert!(!feed.is_valid());

        // после переподключения Bybit шлёт снимок с u = 1 — полный сброс
        let events = feed.apply(input(
            Snapshot,
            BookSeq::Bybit { update_id: 1 },
            &[98.0],
            &[102.0],
        ));
        assert_eq!(kinds(&events), ["gap", "book"]);
        let gap = gaps(&events)[0];
        assert_eq!(gap.last_valid_update_id, 3);
        assert_eq!(gap.first_recovered_update_id, 1);
        let updates = book_updates(&events);
        assert!(updates[0].valid);
        assert_eq!(updates[0].bids, [Level::new(98.0, 1.0)]);
        assert_eq!(updates[0].seq, BookSeq::Bybit { update_id: 1 });
        // нумерация продолжается с нового снимка
        assert_eq!(
            kinds(&feed.apply(input(
                Delta,
                BookSeq::Bybit { update_id: 2 },
                &[97.5, 98.0],
                &[102.0],
            ))),
            ["book"]
        );
        assert!(feed.is_valid());
    }

    #[test]
    fn bybit_snapshot_with_u_not_one_is_dropped_and_invalidates() {
        let mut feed = BookFeed::new(BYBIT);
        feed.apply(input(
            Snapshot,
            BookSeq::Bybit { update_id: 1 },
            &[100.0],
            &[101.0],
        ));
        // снимок без `u == 1` — нарушение протокола: мусор не применяется,
        // но ждём настоящий снимок (u = 1), пока книга невалидна
        let events = feed.apply(input(
            Snapshot,
            BookSeq::Bybit { update_id: 7 },
            &[90.0],
            &[110.0],
        ));
        assert_eq!(kinds(&events), ["book"]);
        assert!(!book_updates(&events)[0].valid);
        assert!(!feed.is_valid());
        // мусор не попал в книгу: содержимое осталось прежним
        assert_eq!(feed.book().bids(), [Level::new(100.0, 1.0)]);
        // настоящий снимок восстанавливает
        let events = feed.apply(input(
            Snapshot,
            BookSeq::Bybit { update_id: 1 },
            &[98.0],
            &[102.0],
        ));
        assert_eq!(kinds(&events), ["gap", "book"]);
        assert!(book_updates(&events)[0].valid);
        assert!(feed.is_valid());
        assert_eq!(feed.book().bids(), [Level::new(98.0, 1.0)]);
    }

    // --- OKX: официальный пример 10 → 15 → 15 → 3 → 5 ---

    #[test]
    fn okx_official_example_walks_through() {
        let mut feed = BookFeed::new(OKX);
        // снимок: prevSeqId = -1
        assert_eq!(
            kinds(&feed.apply(input(
                Snapshot,
                BookSeq::Okx {
                    seq_id: 10,
                    prev_seq_id: -1
                },
                &[100.0],
                &[101.0],
            ))),
            ["book"]
        );
        // инкремент 10 → 15
        assert_eq!(
            kinds(&feed.apply(input(
                Delta,
                BookSeq::Okx {
                    seq_id: 15,
                    prev_seq_id: 10
                },
                &[99.5],
                &[101.0],
            ))),
            ["book"]
        );
        // «нет обновлений»: prevSeqId == seqId, пустые изменения — не событие
        let events = feed.apply(input(
            Delta,
            BookSeq::Okx {
                seq_id: 15,
                prev_seq_id: 15,
            },
            &[],
            &[],
        ));
        assert!(events.is_empty());
        assert!(feed.is_valid());
        // сброс нумерации при обслуживании: 15 → 3, данные непрерывны
        let events = feed.apply(input(
            Delta,
            BookSeq::Okx {
                seq_id: 3,
                prev_seq_id: 15,
            },
            &[99.0],
            &[101.0],
        ));
        assert_eq!(kinds(&events), ["book"]);
        assert!(feed.is_valid());
        // далее обычное правило: 3 → 5
        assert_eq!(
            kinds(&feed.apply(input(
                Delta,
                BookSeq::Okx {
                    seq_id: 5,
                    prev_seq_id: 3
                },
                &[98.5, 99.0],
                &[101.0],
            ))),
            ["book"]
        );
        // дельты меняют только перечисленные уровни: прежние остаются
        assert_eq!(
            feed.book().bids(),
            [
                Level::new(100.0, 1.0),
                Level::new(99.5, 1.0),
                Level::new(99.0, 1.0),
                Level::new(98.5, 1.0),
            ]
        );
    }

    #[test]
    fn okx_gap_invalidates_until_snapshot() {
        let mut feed = BookFeed::new(OKX);
        feed.apply(input(
            Snapshot,
            BookSeq::Okx {
                seq_id: 10,
                prev_seq_id: -1,
            },
            &[100.0],
            &[101.0],
        ));
        feed.apply(input(
            Delta,
            BookSeq::Okx {
                seq_id: 15,
                prev_seq_id: 10,
            },
            &[99.5],
            &[101.0],
        ));

        // разрыв: ждали prevSeqId = 15, пришло 20
        let events = feed.apply(input(
            Delta,
            BookSeq::Okx {
                seq_id: 20,
                prev_seq_id: 17,
            },
            &[100.0],
            &[101.0],
        ));
        assert_eq!(kinds(&events), ["book"]);
        assert!(!book_updates(&events)[0].valid);
        assert!(!feed.is_valid());

        let events = feed.apply(input(
            Snapshot,
            BookSeq::Okx {
                seq_id: 40,
                prev_seq_id: -1,
            },
            &[97.0],
            &[103.0],
        ));
        assert_eq!(kinds(&events), ["gap", "book"]);
        let gap = gaps(&events)[0];
        assert_eq!(gap.last_valid_update_id, 15);
        assert_eq!(gap.first_recovered_update_id, 40);
        assert!(feed.is_valid());
    }

    #[test]
    fn gap_before_first_snapshot_is_not_reported() {
        // дельты до снимка: нечего терять — разрыв с границами не пишется
        let mut feed = BookFeed::new(OKX);
        assert!(
            feed.apply(input(
                Delta,
                BookSeq::Okx {
                    seq_id: 5,
                    prev_seq_id: 4
                },
                &[],
                &[]
            ))
            .is_empty()
        );
        assert!(!feed.is_valid());
        let events = feed.apply(input(
            Snapshot,
            BookSeq::Okx {
                seq_id: 20,
                prev_seq_id: -1,
            },
            &[100.0],
            &[101.0],
        ));
        assert_eq!(kinds(&events), ["book"]);
        assert!(gaps(&events).is_empty());
        assert!(feed.is_valid());
    }

    // --- плановый разрыв и отсоединение ---

    #[test]
    fn planned_gap_emits_event_and_suppresses_gap_at_recovery() {
        let mut feed = BookFeed::new(BINANCE);
        feed.apply(input(Snapshot, b(100, 100, 100), &[100.0], &[101.0]));

        let events = feed.mark_planned_gap(header());
        assert_eq!(kinds(&events), ["planned_gap", "book"]);
        assert!(!book_updates(&events)[0].valid);
        assert!(!feed.is_valid());

        // плановое переподключение: снимок восстанавливает без «разрыва»
        let events = feed.apply(input(Snapshot, b(200, 200, 200), &[99.0], &[102.0]));
        assert_eq!(kinds(&events), ["book"]);
        assert!(gaps(&events).is_empty());
        assert!(book_updates(&events)[0].valid);
        assert!(feed.is_valid());
    }

    #[test]
    fn unplanned_disconnect_reports_gap_with_boundaries_at_recovery() {
        let mut feed = BookFeed::new(BINANCE);
        feed.apply(input(Snapshot, b(100, 100, 100), &[100.0], &[101.0]));
        feed.apply(input(Delta, b(101, 105, 100), &[100.0], &[101.0]));

        // обрыв без объявления: книга инвалидируется сразу, разрыв — при
        // восстановлении
        let events = feed.mark_disconnected(header());
        assert_eq!(kinds(&events), ["book"]);
        assert!(!book_updates(&events)[0].valid);
        assert!(!feed.is_valid());
        // повторная отметка ничего не меняет
        assert!(feed.mark_disconnected(header()).is_empty());

        let events = feed.apply(input(Snapshot, b(500, 500, 500), &[98.0], &[103.0]));
        assert_eq!(kinds(&events), ["gap", "book"]);
        let gap = gaps(&events)[0];
        assert_eq!(gap.last_valid_update_id, 105);
        assert_eq!(gap.first_recovered_update_id, 500);
        assert!(feed.is_valid());
    }

    #[test]
    fn disconnection_before_first_snapshot_emits_nothing() {
        let mut feed = BookFeed::new(BYBIT);
        assert!(feed.mark_disconnected(header()).is_empty());
        assert!(!feed.is_valid());
    }

    // --- лента сделок: пропуск по монотонности id ---

    fn trade(trade_id: i64) -> Trade {
        Trade {
            trade_id,
            price: 100.0,
            qty: 0.5,
            taker_side: crate::event::TradeSide::Buy,
        }
    }

    #[test]
    fn first_trade_sets_baseline_without_gap() {
        let mut feed = TradeFeed::new();
        let events = feed.apply(header(), trade(7));
        assert_eq!(kinds(&events), ["trade"]);
    }

    #[test]
    fn consecutive_trades_produce_no_gap() {
        let mut feed = TradeFeed::new();
        feed.apply(header(), trade(7));
        assert_eq!(kinds(&feed.apply(header(), trade(8))), ["trade"]);
        assert_eq!(kinds(&feed.apply(header(), trade(9))), ["trade"]);
    }

    #[test]
    fn skipped_trades_are_reported_without_invalidating_anything() {
        let mut feed = TradeFeed::new();
        feed.apply(header(), trade(10));
        // пропущены 11 и 12, пришла 13
        let events = feed.apply(header(), trade(13));
        assert_eq!(kinds(&events), ["trade_gap", "trade"]);
        match events[0].kind {
            EventKind::TradeGap(g) => {
                assert_eq!(g.last_trade_id, 10);
                assert_eq!(g.first_trade_id, 13);
                assert_eq!(g.skipped, 2);
            }
            _ => panic!("ожидалось событие «пропуск сделок»"),
        }
    }

    #[test]
    fn trade_id_restart_resets_baseline_without_gap() {
        // сброс нумерации (переподключение, повтор): разрыв не пишется
        let mut feed = TradeFeed::new();
        feed.apply(header(), trade(100));
        assert_eq!(kinds(&feed.apply(header(), trade(3))), ["trade"]);
        assert_eq!(kinds(&feed.apply(header(), trade(4))), ["trade"]);
    }

    // --- приоритет плановости: отсоединение внутри планового окна ---

    #[test]
    fn planned_gap_survives_unplanned_disconnect_mark() {
        // плановое переподключение уже помечено: обрыв потока внутри планового
        // окна не делает разрыв внеплановым — при восстановлении Gap не
        // докладывается (коннектор #18 кладёт PlannedGap до отключения)
        let mut feed = BookFeed::new(BINANCE);
        feed.apply(input(Snapshot, b(100, 100, 100), &[100.0], &[101.0]));
        assert_eq!(
            kinds(&feed.mark_planned_gap(header())),
            ["planned_gap", "book"]
        );
        assert!(feed.mark_disconnected(header()).is_empty());

        let events = feed.apply(input(Snapshot, b(200, 200, 200), &[99.0], &[102.0]));
        assert_eq!(kinds(&events), ["book"]);
        assert!(gaps(&events).is_empty());
        assert!(book_updates(&events)[0].valid);
        assert!(feed.is_valid());
    }
}
