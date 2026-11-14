//! Интеграционный roundtrip-тест хранилища: события → writer → reader →
//! те же события (критерий готовности тикета #16). Всё через публичный API
//! крейта: настоящие файлы во временной директории, ротация дней, retention.

use alko_core::book::Level;
use alko_core::event::{BookUpdate, Event, EventHeader, EventKind, Gap, TradeGap};
use alko_core::{BookSeq, Pair, Timestamps, Trade, TradeSide};
use alko_store::{Exchange, Writer, prune, read_events};

const DAY_1_US: i64 = 20_468 * alko_store::US_PER_DAY; // 2026-01-15 12:00 UTC
const DAY_2_US: i64 = DAY_1_US + alko_store::US_PER_DAY;

fn header(pair: Pair, ts: Timestamps, offset: i64) -> EventHeader {
    EventHeader {
        pair,
        ts,
        clock_offset_us: offset,
    }
}

fn ts(event: i64, push: Option<i64>, received: i64) -> Timestamps {
    Timestamps {
        event: Some(event),
        push,
        received: Some(received),
        delivered: None, // проставляет писатель (ADR 0005: очередь потребителя)
    }
}

/// Сутки записи одной пары на одной бирже: книга всех трёх схем после
/// разрыва и восстановления, сделки с пропуском, плановый разрыв.
fn recording_of(pair: Pair, exchange: Exchange) -> Vec<Event> {
    let offset = match exchange {
        Exchange::Binance => -200,
        Exchange::Bybit => -2_000,
        Exchange::Okx => 2_500,
    };
    let push = match exchange {
        // ADR 0005: у книги Binance отдельного пуша нет; у сделок Bybit тоже
        Exchange::Binance => None,
        _ => Some(1),
    };
    vec![
        // невалидная книга после разрыва: содержимое устаревшее, флаг false
        Event {
            header: header(pair, ts(100, push, 1_000), offset),
            kind: EventKind::BookUpdate(BookUpdate {
                bids: vec![Level::new(99_999.0, 2.0)],
                asks: vec![Level::new(100_001.0, 1.0), Level::new(100_002.0, 3.0)],
                valid: false,
                seq: BookSeq::Binance {
                    first: 50,
                    last: 60,
                    prev_last: 60,
                },
            }),
        },
        // разрыв с границами вокруг дыры
        Event {
            header: header(pair, ts(200, push, 2_000), offset),
            kind: EventKind::Gap(Gap {
                last_valid_update_id: 60,
                first_recovered_update_id: 205,
            }),
        },
        // снимок, вернувший валидность: пустая сторона и полная книга 20×20
        Event {
            header: header(pair, ts(300, push, 3_000), offset),
            kind: EventKind::BookUpdate(BookUpdate {
                bids: (0..20)
                    .map(|i| Level::new(99_999.0 - i as f64, 1.0 + i as f64))
                    .collect(),
                asks: vec![],
                valid: true,
                seq: BookSeq::Bybit { update_id: 205 },
            }),
        },
        // сырые сделки (ADR 0006) обеих сторон, в том числе с крошечными ценами
        Event {
            header: header(pair, ts(400, Some(400), 4_000), offset),
            kind: EventKind::Trade(Trade {
                trade_id: 1_000,
                price: 100_000.5,
                qty: 0.000_001,
                taker_side: TradeSide::Buy,
            }),
        },
        Event {
            header: header(pair, ts(401, Some(401), 4_100), offset),
            kind: EventKind::Trade(Trade {
                trade_id: 1_001,
                price: 99_999.25,
                qty: 5.5,
                taker_side: TradeSide::Sell,
            }),
        },
        // пропуск сделок: 1_005..1_010 не дошли
        Event {
            header: header(pair, ts(500, Some(500), 5_000), offset),
            kind: EventKind::TradeGap(TradeGap {
                last_trade_id: 1_004,
                first_trade_id: 1_010,
                skipped: 5,
            }),
        },
        // плановый разрыв без payload
        Event {
            header: header(pair, ts(600, None, 6_000), offset),
            kind: EventKind::PlannedGap,
        },
        // восстановление книги OKX: сброс нумерации при обслуживании
        Event {
            header: header(pair, ts(700, None, 7_000), offset),
            kind: EventKind::BookUpdate(BookUpdate {
                bids: vec![],
                asks: vec![Level::new(0.000_001, 7.5)],
                valid: true,
                seq: BookSeq::Okx {
                    seq_id: 3,
                    prev_seq_id: 15,
                },
            }),
        },
    ]
}

/// События → writer → reader → те же события: все пять видов, все три
/// схемы последовательностей, обе границы суток, все три биржи.
#[test]
fn roundtrip_writer_reader_preserves_events() {
    let dir = tempfile::tempdir().unwrap();

    // каждая биржа — свой поток; события без tsDelivered получают его от
    // писателя, с готовой — сохраняют
    for exchange in [Exchange::Binance, Exchange::Bybit, Exchange::Okx] {
        let pair = Pair::BTCUSDT;
        let mut writer = Writer::new(dir.path(), pair, exchange);
        for (i, mut event) in recording_of(pair, exchange).into_iter().enumerate() {
            // первая половина событий пишется в первый день, вторая — во
            // второй: ротация по tsDelivered только вперёд
            let now = DAY_1_US + (i / 4) as i64 * alko_store::US_PER_DAY + i as i64;
            writer.write(&mut event, now).unwrap();
        }
        writer.finish().unwrap();
    }

    // чтение каждого файла: события совпадают с записанными с точностью
    // до проставленной писателем tsDelivered
    for exchange in [Exchange::Binance, Exchange::Bybit, Exchange::Okx] {
        let pair = Pair::BTCUSDT;
        let recorded = recording_of(pair, exchange);
        let day1_path = dir
            .path()
            .join(exchange.as_str())
            .join(pair.as_str())
            .join("2026-01-15.parquet");
        let day2_path = dir
            .path()
            .join(exchange.as_str())
            .join(pair.as_str())
            .join("2026-01-16.parquet");
        let mut read = read_events(&day1_path).unwrap();
        read.extend(read_events(&day2_path).unwrap());
        assert_eq!(read.len(), recorded.len());
        for (written, mut expected) in read.into_iter().zip(recorded) {
            expected.header.ts.delivered = written.header.ts.delivered;
            assert_eq!(written, expected);
        }
    }
}

/// Готовая tsDelivered сохраняется, а не подменяется моментом записи.
#[test]
fn pre_stamped_delivered_survives_the_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let stamp = DAY_2_US - 500_000; // 23:59:59.5 первого дня: ещё 2026-01-15
    let mut event = Event {
        header: header(
            Pair::ZECUSDT,
            Timestamps {
                event: Some(1),
                push: None,
                received: Some(2),
                delivered: Some(stamp),
            },
            42,
        ),
        kind: EventKind::PlannedGap,
    };
    // now — уже следующие сутки: день файла определяет готовая метка
    let mut writer = Writer::new(dir.path(), Pair::ZECUSDT, Exchange::Binance);
    writer.write(&mut event, DAY_2_US).unwrap();
    writer.finish().unwrap();

    let path = dir
        .path()
        .join("binance")
        .join("ZECUSDT")
        .join("2026-01-15.parquet");
    let events = read_events(&path).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].header.ts.delivered, Some(stamp));
}

/// Retention: файлы старше 30 дней удаляются, граница — строго больше
/// retention, чужие имена не трогаются (критерий готовности #16).
#[test]
fn retention_deletes_files_older_than_thirty_days() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let make = |exchange: &str, pair: &str, day: i64| {
        let path = root
            .join(exchange)
            .join(pair)
            .join(format!("{}.parquet", alko_store::format_day(day)));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"x").unwrap();
        path
    };
    let today = alko_store::day_of(DAY_2_US);
    let fresh = make("binance", "BTCUSDT", today);
    let boundary = make("binance", "BTCUSDT", today - 30); // возраст ровно 30: остаётся
    let stale = make("binance", "BTCUSDT", today - 31); // старше 30: удаляется
    let stale_bybit = make("bybit", "BTCUSDT", today - 100);
    let foreign = root.join("binance").join("BTCUSDT").join("notes.txt");
    std::fs::write(&foreign, "не запись").unwrap();

    let deleted = prune(root, DAY_2_US, 30).unwrap();
    assert_eq!(deleted.len(), 2);
    assert!(deleted.contains(&stale));
    assert!(deleted.contains(&stale_bybit));
    assert!(!stale.exists());
    assert!(!stale_bybit.exists());
    assert!(fresh.exists());
    assert!(boundary.exists());
    assert!(foreign.exists(), "чужие файлы retention не трогает");
}

/// Retention по несуществующей директории — не ошибка: pruning до первой
/// записи штатен.
#[test]
fn retention_on_empty_dir_is_noop() {
    let dir = tempfile::tempdir().unwrap();
    let deleted = prune(dir.path(), DAY_1_US, 30).unwrap();
    assert!(deleted.is_empty());
}
