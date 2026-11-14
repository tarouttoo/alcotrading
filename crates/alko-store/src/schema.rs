//! Схема Parquet — контракт записи (спека #13, ADR 0005/0006). Один модуль
//! описывает колонки и кодирование/декодирование строк, им пользуются и
//! writer, и reader: схема не может разъехаться между ними.
//!
//! Одна строка файла — одно событие ([`Event`], Q31) одного стека
//! «пара × биржа» за один UTC-день. Файл — `{data_dir}/{exchange}/{pair}/
//! {YYYY-MM-DD}.parquet`: под «один файл на пару и день» спеки #13 биржа —
//! часть пути потока, потому что событие не несёт биржу в заголовке
//! ([`EventHeader`]), а писатель привязан к потоку целиком.
//!
//! Колонки — плоские, nullable-колонки честно пусты там, где поле к событию
//! не относится (ADR 0005: «честный пропуск вместо дублирования»): вид
//! события различается колонкой `kind`, а не отдельными файлами или JSON.
//! Никакого сырого JSON (спека #13, Q48) и никаких float-меток времени
//! (Q39): только int64 микросекунды UTC.

use alko_core::book::Level;
use alko_core::event::{BookUpdate, Event, EventHeader, EventKind, Gap, TradeGap};
use alko_core::{BookSeq, Pair, Timestamps, Trade, TradeSide};

use arrow::array::{
    Array, ArrayRef, BooleanArray, Float64Array, Int64Array, ListArray, RecordBatch, StringArray,
    StructArray,
};
use arrow::buffer::{NullBuffer, OffsetBuffer};
use arrow::datatypes::{DataType, Field, Fields, Schema, SchemaRef};
use std::sync::Arc;

/// Биржа потока: значение колонки `exchange`, часть пути файла и
/// параметр писателя (заголовок события биржу не несёт).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exchange {
    /// Binance USDⓈ-M.
    Binance,
    /// Bybit derivatives.
    Bybit,
    /// OKX.
    Okx,
}

impl Exchange {
    /// Каноническое имя биржи в схеме и пути записи.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Binance => "binance",
            Self::Bybit => "bybit",
            Self::Okx => "okx",
        }
    }

    /// Разобрать каноническое имя; `None` для всего прочего.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "binance" => Some(Self::Binance),
            "bybit" => Some(Self::Bybit),
            "okx" => Some(Self::Okx),
            _ => None,
        }
    }
}

impl std::fmt::Display for Exchange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Вид события: значение колонки `kind`.
fn kind_str(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::BookUpdate(_) => "book_update",
        EventKind::Trade(_) => "trade",
        EventKind::Gap(_) => "gap",
        EventKind::PlannedGap => "planned_gap",
        EventKind::TradeGap(_) => "trade_gap",
    }
}

/// Поля структуры уровня стакана: цена и абсолютное количество.
fn level_fields() -> Fields {
    Fields::from([
        Arc::new(Field::new("price", DataType::Float64, false)),
        Arc::new(Field::new("qty", DataType::Float64, false)),
    ])
}

/// Проверка схемы файла или батча на соответствие контракту [`schema`]:
/// имена, типы и nullability колонок — по порядку, включая вложенные поля
/// структур уровня. Файл чужой схемы — не наша запись, даже если в нём нет
/// ни одной строки: имя-совместимый файл с nullable ценой уровня иначе
/// декодируется с молча обнулённой ценой.
pub(crate) fn ensure_schema(actual: &SchemaRef) -> Result<(), crate::StoreError> {
    if actual.fields() != schema().fields() {
        return Err(crate::StoreError::Schema(
            "схема файла не совпадает со схемой хранилища".to_string(),
        ));
    }
    Ok(())
}

/// Схема Parquet-файла: единственное описание колонок для writer и reader.
/// Порядок и nullability колонок зафиксированы тестами выше — это контракт
/// датасета, читаемого годами (перестроение схемы после старта записи
/// болезненно, см. ADR 0005).
pub fn schema() -> SchemaRef {
    let field =
        |name: &str, dtype: DataType, nullable: bool| Arc::new(Field::new(name, dtype, nullable));
    let int64 = |name: &str, nullable: bool| field(name, DataType::Int64, nullable);
    let utf8 = |name: &str, nullable: bool| field(name, DataType::Utf8, nullable);
    let levels = |name: &str| {
        field(
            name,
            DataType::List(Arc::new(Field::new(
                "level",
                DataType::Struct(level_fields()),
                false,
            ))),
            true,
        )
    };
    Arc::new(Schema::new(vec![
        // Заголовок события: вид, пара, биржа потока.
        utf8("kind", false),
        utf8("pair", false),
        utf8("exchange", false),
        // Метки PERD: nullable int64 мкc UTC (ADR 0005).
        int64("ts_event", true),
        int64("ts_push", true),
        int64("ts_received", true),
        int64("ts_delivered", true),
        // Поправка часов биржи: последнее измеренное значение (ADR 0005).
        int64("exchange_clock_offset_us", false),
        // Метка последовательности применённого состояния книги (BookSeq).
        utf8("seq_scheme", true),
        int64("seq_binance_first", true),
        int64("seq_binance_last", true),
        int64("seq_binance_prev_last", true),
        int64("seq_bybit_update_id", true),
        int64("seq_okx_seq_id", true),
        int64("seq_okx_prev_seq_id", true),
        // Payload обновления книги: флаг валидности и топ-20 уровней.
        field("book_valid", DataType::Boolean, true),
        levels("book_bids"),
        levels("book_asks"),
        // Payload сырой сделки с нативным идентификатором (ADR 0006).
        int64("trade_id", true),
        field("trade_price", DataType::Float64, true),
        field("trade_qty", DataType::Float64, true),
        utf8("trade_taker_side", true),
        // Границы разрыва книги: фактические идентификаторы вокруг дыры.
        int64("gap_last_valid_update_id", true),
        int64("gap_first_recovered_update_id", true),
        // Границы пропуска сделок (ADR 0006).
        int64("trade_gap_last_trade_id", true),
        int64("trade_gap_first_trade_id", true),
        int64("trade_gap_skipped", true),
    ]))
}

/// Значение колонки `trade_taker_side` для стороны тейкера.
fn side_str(side: TradeSide) -> &'static str {
    match side {
        TradeSide::Buy => "buy",
        TradeSide::Sell => "sell",
    }
}

/// Схема последовательности книги: значение колонки `seq_scheme`.
fn scheme_str(seq: &BookSeq) -> &'static str {
    match seq {
        BookSeq::Binance { .. } => "binance",
        BookSeq::Bybit { .. } => "bybit",
        BookSeq::Okx { .. } => "okx",
    }
}

/// Уровни стакана: список структур {price, qty}; `None` — nullable-колонка
/// события, к книге не относящегося (ADR 0005: честный пропуск).
fn levels_array(levels: Option<&[Level]>) -> ListArray {
    let elem = Arc::new(Field::new("level", DataType::Struct(level_fields()), false));
    let (offsets, values, nulls) = match levels {
        Some(ls) => {
            let prices: Float64Array = ls.iter().map(|l| l.price).collect();
            let qtys: Float64Array = ls.iter().map(|l| l.qty).collect();
            let values =
                StructArray::new(level_fields(), vec![Arc::new(prices), Arc::new(qtys)], None);
            (
                OffsetBuffer::new(vec![0_i32, ls.len() as i32].into()),
                Arc::new(values) as ArrayRef,
                None,
            )
        }
        None => {
            let values = StructArray::new(
                level_fields(),
                vec![
                    Arc::new(Float64Array::from(Vec::<f64>::new())),
                    Arc::new(Float64Array::from(Vec::<f64>::new())),
                ],
                None,
            );
            (
                OffsetBuffer::new(vec![0_i32, 0].into()),
                Arc::new(values) as ArrayRef,
                Some(NullBuffer::from(vec![false])),
            )
        }
    };
    ListArray::new(elem, offsets, values, nulls)
}

/// Кодировать событие в батч из одной строки по схеме [`schema`]. Биржа —
/// параметр: заголовок события её не несёт, её знает привязанный к потоку
/// писатель.
pub fn event_to_batch(event: &Event, exchange: Exchange) -> Result<RecordBatch, crate::StoreError> {
    let Event { header, kind } = event;
    let ts = &header.ts;
    let (seq, book, trade, gap, trade_gap) = match kind {
        EventKind::BookUpdate(b) => (Some(&b.seq), Some(b), None, None, None),
        EventKind::Trade(t) => (None, None, Some(t), None, None),
        EventKind::Gap(g) => (None, None, None, Some(g), None),
        EventKind::PlannedGap => (None, None, None, None, None),
        EventKind::TradeGap(tg) => (None, None, None, None, Some(tg)),
    };
    let (mut s_bf, mut s_bl, mut s_bp, mut s_bu, mut s_os, mut s_op) =
        (None, None, None, None, None, None);
    if let Some(seq) = seq {
        match seq {
            BookSeq::Binance {
                first,
                last,
                prev_last,
            } => {
                s_bf = Some(*first);
                s_bl = Some(*last);
                s_bp = Some(*prev_last);
            }
            BookSeq::Bybit { update_id } => s_bu = Some(*update_id),
            BookSeq::Okx {
                seq_id,
                prev_seq_id,
            } => {
                s_os = Some(*seq_id);
                s_op = Some(*prev_seq_id);
            }
        }
    }
    let scheme = seq.map(scheme_str);
    let side = trade.map(|t| side_str(t.taker_side));

    let cols: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(vec![kind_str(kind)])),
        Arc::new(StringArray::from(vec![header.pair.as_str()])),
        Arc::new(StringArray::from(vec![exchange.as_str()])),
        Arc::new(Int64Array::from(vec![ts.event])),
        Arc::new(Int64Array::from(vec![ts.push])),
        Arc::new(Int64Array::from(vec![ts.received])),
        Arc::new(Int64Array::from(vec![ts.delivered])),
        Arc::new(Int64Array::from(vec![header.clock_offset_us])),
        Arc::new(StringArray::from(vec![scheme])),
        Arc::new(Int64Array::from(vec![s_bf])),
        Arc::new(Int64Array::from(vec![s_bl])),
        Arc::new(Int64Array::from(vec![s_bp])),
        Arc::new(Int64Array::from(vec![s_bu])),
        Arc::new(Int64Array::from(vec![s_os])),
        Arc::new(Int64Array::from(vec![s_op])),
        Arc::new(BooleanArray::from(vec![book.map(|b| b.valid)])),
        Arc::new(levels_array(book.map(|b| b.bids.as_slice()))),
        Arc::new(levels_array(book.map(|b| b.asks.as_slice()))),
        Arc::new(Int64Array::from(vec![trade.map(|t| t.trade_id)])),
        Arc::new(Float64Array::from(vec![trade.map(|t| t.price)])),
        Arc::new(Float64Array::from(vec![trade.map(|t| t.qty)])),
        Arc::new(StringArray::from(vec![side])),
        Arc::new(Int64Array::from(vec![gap.map(|g| g.last_valid_update_id)])),
        Arc::new(Int64Array::from(vec![
            gap.map(|g| g.first_recovered_update_id),
        ])),
        Arc::new(Int64Array::from(vec![trade_gap.map(|g| g.last_trade_id)])),
        Arc::new(Int64Array::from(vec![trade_gap.map(|g| g.first_trade_id)])),
        Arc::new(Int64Array::from(vec![trade_gap.map(|g| g.skipped)])),
    ];
    Ok(RecordBatch::try_new(schema(), cols)?)
}

// --- декодирование ---

/// Типизированная колонка батча; расхождение — ошибка схемы.
fn typed<'a, A: Array + 'static>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a A, crate::StoreError> {
    batch
        .column_by_name(name)
        .ok_or_else(|| crate::StoreError::Schema(format!("нет колонки {name}")))?
        .as_any()
        .downcast_ref::<A>()
        .ok_or_else(|| crate::StoreError::Schema(format!("колонка {name} другого типа")))
}

fn opt_i64(batch: &RecordBatch, name: &str, i: usize) -> Result<Option<i64>, crate::StoreError> {
    let col = typed::<Int64Array>(batch, name)?;
    Ok((!col.is_null(i)).then(|| col.value(i)))
}

/// Обязательное значение: null в payload-колонке при виде события,
/// которому оно нужно, — битый файл, молча восстанавливать нельзя.
fn req_i64(batch: &RecordBatch, name: &str, i: usize) -> Result<i64, crate::StoreError> {
    opt_i64(batch, name, i)?
        .ok_or_else(|| crate::StoreError::Schema(format!("пусто в {name}, строка {i}")))
}

fn req_f64(batch: &RecordBatch, name: &str, i: usize) -> Result<f64, crate::StoreError> {
    let col = typed::<Float64Array>(batch, name)?;
    if col.is_null(i) {
        return Err(crate::StoreError::Schema(format!(
            "пусто в {name}, строка {i}"
        )));
    }
    Ok(col.value(i))
}

fn req_str<'a>(batch: &'a RecordBatch, name: &str, i: usize) -> Result<&'a str, crate::StoreError> {
    let col = typed::<StringArray>(batch, name)?;
    if col.is_null(i) {
        return Err(crate::StoreError::Schema(format!(
            "пусто в {name}, строка {i}"
        )));
    }
    Ok(col.value(i))
}

fn req_bool(batch: &RecordBatch, name: &str, i: usize) -> Result<bool, crate::StoreError> {
    let col = typed::<BooleanArray>(batch, name)?;
    if col.is_null(i) {
        return Err(crate::StoreError::Schema(format!(
            "пусто в {name}, строка {i}"
        )));
    }
    Ok(col.value(i))
}

fn levels(batch: &RecordBatch, name: &str, i: usize) -> Result<Vec<Level>, crate::StoreError> {
    let list = typed::<ListArray>(batch, name)?;
    if list.is_null(i) {
        return Ok(Vec::new());
    }
    let slice = list.value(i);
    let st = slice
        .as_any()
        .downcast_ref::<StructArray>()
        .ok_or_else(|| crate::StoreError::Schema(format!("{name}: уровень не структура")))?;
    let prices = st
        .column(0)
        .as_any()
        .downcast_ref::<Float64Array>()
        .ok_or_else(|| crate::StoreError::Schema(format!("{name}: цена не float64")))?;
    let qtys = st
        .column(1)
        .as_any()
        .downcast_ref::<Float64Array>()
        .ok_or_else(|| crate::StoreError::Schema(format!("{name}: количество не float64")))?;
    Ok((0..st.len())
        .map(|j| Level::new(prices.value(j), qtys.value(j)))
        .collect())
}

fn side(s: &str) -> Result<TradeSide, crate::StoreError> {
    match s {
        "buy" => Ok(TradeSide::Buy),
        "sell" => Ok(TradeSide::Sell),
        other => Err(crate::StoreError::Schema(format!(
            "неизвестная сторона сделки {other}"
        ))),
    }
}

/// Декодировать батч, записанный [`event_to_batch`], обратно в события —
/// по строке на событие, в порядке строк файла.
pub fn batch_to_events(batch: &RecordBatch) -> Result<Vec<Event>, crate::StoreError> {
    // тот же контракт, что закреплён тестами схемы; чужой батч не
    // декодируется молча
    ensure_schema(&batch.schema())?;

    let mut events = Vec::with_capacity(batch.num_rows());
    for i in 0..batch.num_rows() {
        let pair_str = req_str(batch, "pair", i)?;
        let pair = Pair::ALL
            .iter()
            .copied()
            .find(|p| p.as_str() == pair_str)
            .ok_or_else(|| crate::StoreError::Schema(format!("неизвестная пара {pair_str}")))?;
        let header = EventHeader {
            pair,
            ts: Timestamps {
                event: opt_i64(batch, "ts_event", i)?,
                push: opt_i64(batch, "ts_push", i)?,
                received: opt_i64(batch, "ts_received", i)?,
                delivered: opt_i64(batch, "ts_delivered", i)?,
            },
            clock_offset_us: req_i64(batch, "exchange_clock_offset_us", i)?,
        };
        let kind = match req_str(batch, "kind", i)? {
            "book_update" => {
                let seq = match req_str(batch, "seq_scheme", i)? {
                    "binance" => BookSeq::Binance {
                        first: req_i64(batch, "seq_binance_first", i)?,
                        last: req_i64(batch, "seq_binance_last", i)?,
                        prev_last: req_i64(batch, "seq_binance_prev_last", i)?,
                    },
                    "bybit" => BookSeq::Bybit {
                        update_id: req_i64(batch, "seq_bybit_update_id", i)?,
                    },
                    "okx" => BookSeq::Okx {
                        seq_id: req_i64(batch, "seq_okx_seq_id", i)?,
                        prev_seq_id: req_i64(batch, "seq_okx_prev_seq_id", i)?,
                    },
                    other => {
                        return Err(crate::StoreError::Schema(format!(
                            "неизвестная схема последовательности {other}"
                        )));
                    }
                };
                EventKind::BookUpdate(BookUpdate {
                    bids: levels(batch, "book_bids", i)?,
                    asks: levels(batch, "book_asks", i)?,
                    valid: req_bool(batch, "book_valid", i)?,
                    seq,
                })
            }
            "trade" => EventKind::Trade(Trade {
                trade_id: req_i64(batch, "trade_id", i)?,
                price: req_f64(batch, "trade_price", i)?,
                qty: req_f64(batch, "trade_qty", i)?,
                taker_side: side(req_str(batch, "trade_taker_side", i)?)?,
            }),
            "gap" => EventKind::Gap(Gap {
                last_valid_update_id: req_i64(batch, "gap_last_valid_update_id", i)?,
                first_recovered_update_id: req_i64(batch, "gap_first_recovered_update_id", i)?,
            }),
            "planned_gap" => EventKind::PlannedGap,
            "trade_gap" => EventKind::TradeGap(TradeGap {
                last_trade_id: req_i64(batch, "trade_gap_last_trade_id", i)?,
                first_trade_id: req_i64(batch, "trade_gap_first_trade_id", i)?,
                skipped: req_i64(batch, "trade_gap_skipped", i)?,
            }),
            other => {
                return Err(crate::StoreError::Schema(format!(
                    "неизвестный вид события {other}"
                )));
            }
        };
        events.push(Event { header, kind });
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Схема — контракт ADR 0005/0006: имена колонок по порядку, типы и
    /// nullability. Порядок — часть контракта датасета (файлы читаются
    /// годами), он зафиксирован тестом.
    #[test]
    fn schema_columns_match_adr_0005() {
        let schema = schema();
        let names: Vec<&str> = schema
            .fields()
            .iter()
            .map(|field| field.name().as_str())
            .collect();
        assert_eq!(
            names,
            [
                // заголовок события (EventHeader)
                "kind",
                "pair",
                "exchange",
                // четыре метки PERD + поправка часов (ADR 0005)
                "ts_event",
                "ts_push",
                "ts_received",
                "ts_delivered",
                "exchange_clock_offset_us",
                // метка последовательности книги (BookSeq, тикет #15)
                "seq_scheme",
                "seq_binance_first",
                "seq_binance_last",
                "seq_binance_prev_last",
                "seq_bybit_update_id",
                "seq_okx_seq_id",
                "seq_okx_prev_seq_id",
                // payload обновления книги
                "book_valid",
                "book_bids",
                "book_asks",
                // payload сырой сделки (ADR 0006)
                "trade_id",
                "trade_price",
                "trade_qty",
                "trade_taker_side",
                // границы разрыва книги
                "gap_last_valid_update_id",
                "gap_first_recovered_update_id",
                // границы пропуска сделок (ADR 0006)
                "trade_gap_last_trade_id",
                "trade_gap_first_trade_id",
                "trade_gap_skipped",
            ]
        );
    }

    /// ADR 0005: четыре метки — nullable int64 мкс; поправка часов —
    /// обязательная int64 (заголовок события несёт её всегда).
    #[test]
    fn perd_columns_are_nullable_int64() {
        let schema = schema();
        for name in ["ts_event", "ts_push", "ts_received", "ts_delivered"] {
            let field = schema.field_with_name(name).unwrap();
            assert_eq!(field.data_type(), &DataType::Int64, "{name}");
            assert!(field.is_nullable(), "{name} — nullable (ADR 0005)");
        }
        let offset = schema.field_with_name("exchange_clock_offset_us").unwrap();
        assert_eq!(offset.data_type(), &DataType::Int64);
        assert!(!offset.is_nullable(), "поправка есть в каждом EventHeader");
    }

    /// Служебные колонки (`kind`/`pair`/`exchange`) — обязательные строки:
    /// строка без вида, пары или биржи не декодируется в событие.
    #[test]
    fn key_columns_are_non_nullable_strings() {
        let schema = schema();
        for name in ["kind", "pair", "exchange", "seq_scheme"] {
            let field = schema.field_with_name(name).unwrap();
            assert_eq!(field.data_type(), &DataType::Utf8, "{name}");
        }
        for name in ["kind", "pair", "exchange"] {
            assert!(
                !schema.field_with_name(name).unwrap().is_nullable(),
                "{name} — обязательная"
            );
        }
        assert!(
            schema.field_with_name("seq_scheme").unwrap().is_nullable(),
            "seq_scheme пуст у событий без книги"
        );
    }

    /// Уровни книги — список структур {price, qty}; nullable у событий,
    /// не относящихся к книге.
    #[test]
    fn level_columns_are_lists_of_price_qty_structs() {
        let schema = schema();
        for name in ["book_bids", "book_asks"] {
            let field = schema.field_with_name(name).unwrap();
            assert!(field.is_nullable(), "{name} пуст у событий без книги");
            let DataType::List(elem) = field.data_type() else {
                panic!("{name} — список");
            };
            assert_eq!(elem.name(), "level");
            let DataType::Struct(levels) = elem.data_type() else {
                panic!("{name} — список структур");
            };
            let prices: Vec<(&str, &DataType)> = levels
                .iter()
                .map(|f| (f.name().as_str(), f.data_type()))
                .collect();
            assert_eq!(
                prices,
                [("price", &DataType::Float64), ("qty", &DataType::Float64)]
            );
        }
    }

    /// Payload-колонки видов событий — nullable: у строки другого вида они
    /// честно пусты (ADR 0005).
    #[test]
    fn payload_columns_are_nullable() {
        let schema = schema();
        for name in [
            "seq_binance_first",
            "seq_binance_last",
            "seq_binance_prev_last",
            "seq_bybit_update_id",
            "seq_okx_seq_id",
            "seq_okx_prev_seq_id",
            "book_valid",
            "trade_id",
            "trade_price",
            "trade_qty",
            "trade_taker_side",
            "gap_last_valid_update_id",
            "gap_first_recovered_update_id",
            "trade_gap_last_trade_id",
            "trade_gap_first_trade_id",
            "trade_gap_skipped",
        ] {
            let field = schema.field_with_name(name).unwrap();
            assert!(field.is_nullable(), "{name}");
        }
        assert_eq!(
            schema.field_with_name("book_valid").unwrap().data_type(),
            &DataType::Boolean
        );
        assert_eq!(
            schema.field_with_name("trade_price").unwrap().data_type(),
            &DataType::Float64
        );
        assert_eq!(
            schema
                .field_with_name("trade_taker_side")
                .unwrap()
                .data_type(),
            &DataType::Utf8
        );
    }

    // --- кодирование строк: событие ↔ батч ↔ событие ---

    fn ts_full() -> Timestamps {
        Timestamps {
            event: Some(1_700_000_000_000_001),
            push: Some(1_700_000_000_000_002),
            received: Some(1_700_000_000_000_003),
            delivered: Some(1_700_000_000_000_004),
        }
    }

    fn ts_sparse() -> Timestamps {
        // честные пропуски ADR 0005: у книги Binance нет отдельного пуша,
        // delivered проставляет очередь потребителя, не коннектор
        Timestamps {
            event: Some(1_700_000_000_000_005),
            push: None,
            received: Some(1_700_000_000_000_006),
            delivered: None,
        }
    }

    fn header(pair: Pair, ts: Timestamps) -> EventHeader {
        EventHeader {
            pair,
            ts,
            clock_offset_us: -2_500,
        }
    }

    /// Каждые вид события и каждая схема последовательности книги проходят
    /// кодирование и декодирование без потерь — основа roundtrip-теста
    /// «события → writer → reader → те же события» (#16).
    #[test]
    fn codec_roundtrips_every_kind_and_scheme() {
        let events = vec![
            // книга: все три схемы последовательностей, пустая и полная сторона
            Event {
                header: header(Pair::BTCUSDT, ts_full()),
                kind: EventKind::BookUpdate(BookUpdate {
                    bids: vec![Level::new(99_999.5, 1.25), Level::new(99_999.0, 0.5)],
                    asks: vec![],
                    valid: true,
                    seq: BookSeq::Binance {
                        first: 1,
                        last: 2,
                        prev_last: 1,
                    },
                }),
            },
            Event {
                header: header(Pair::ETHUSDT, ts_sparse()),
                kind: EventKind::BookUpdate(BookUpdate {
                    bids: vec![],
                    asks: vec![Level::new(3_000.25, f64::MAX)],
                    valid: false,
                    seq: BookSeq::Bybit { update_id: 7 },
                }),
            },
            Event {
                header: header(Pair::SOLUSDT, ts_full()),
                kind: EventKind::BookUpdate(BookUpdate {
                    bids: (0..20)
                        .map(|i| Level::new(200.0 - i as f64, i as f64))
                        .collect(),
                    asks: (0..20).map(|i| Level::new(200.1 + i as f64, 1.0)).collect(),
                    valid: true,
                    seq: BookSeq::Okx {
                        seq_id: -1,
                        prev_seq_id: 4_503_599_627_049_625,
                    },
                }),
            },
            // сырая сделка с нативным идентификатором (ADR 0006)
            Event {
                header: header(Pair::XRPUSDT, ts_full()),
                kind: EventKind::Trade(Trade {
                    trade_id: 123_456_789,
                    price: 2.5,
                    qty: 100.75,
                    taker_side: TradeSide::Buy,
                }),
            },
            Event {
                header: header(Pair::ZECUSDT, ts_sparse()),
                kind: EventKind::Trade(Trade {
                    trade_id: -1,
                    price: 0.000_001,
                    qty: 0.000_000_1,
                    taker_side: TradeSide::Sell,
                }),
            },
            // разрыв книги с границами «вокруг дыры»
            Event {
                header: header(Pair::BTCUSDT, ts_sparse()),
                kind: EventKind::Gap(Gap {
                    last_valid_update_id: 100,
                    first_recovered_update_id: 205,
                }),
            },
            // плановый разрыв: payload-колонки пусты
            Event {
                header: header(Pair::BTCUSDT, ts_full()),
                kind: EventKind::PlannedGap,
            },
            // пропуск сделок
            Event {
                header: header(Pair::ZECUSDT, ts_sparse()),
                kind: EventKind::TradeGap(TradeGap {
                    last_trade_id: 10,
                    first_trade_id: 13,
                    skipped: 2,
                }),
            },
        ];
        for exchange in [Exchange::Binance, Exchange::Bybit, Exchange::Okx] {
            for event in &events {
                let batch = event_to_batch(event, exchange).unwrap();
                assert_eq!(batch.num_rows(), 1);
                assert_eq!(batch.schema().fields(), schema().fields());
                let back = batch_to_events(&batch).unwrap();
                assert_eq!(back, vec![event.clone()], "для {exchange}");
            }
        }
    }

    /// Колонка exchange несёт биржу писателя: файл самодостаточен для
    /// кросс-бирженного анализа (ADR 0005) даже без пути.
    #[test]
    fn exchange_column_carries_the_writer_binding() {
        let event = Event {
            header: header(Pair::BTCUSDT, ts_full()),
            kind: EventKind::PlannedGap,
        };
        for exchange in [Exchange::Binance, Exchange::Bybit, Exchange::Okx] {
            let batch = event_to_batch(&event, exchange).unwrap();
            let col = batch
                .column_by_name("exchange")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            assert_eq!(col.value(0), exchange.as_str());
        }
        assert_eq!(Exchange::parse("binance"), Some(Exchange::Binance));
        assert_eq!(Exchange::parse("okx"), Some(Exchange::Okx));
        assert_eq!(Exchange::parse("coinbase"), None);
    }

    /// Чужой или битый батч не декодируется молча: расхождение схемы,
    /// неизвестный вид события и отсутствующий payload — ошибки.
    #[test]
    fn decode_rejects_foreign_and_corrupt_batches() {
        let event = Event {
            header: header(Pair::BTCUSDT, ts_full()),
            kind: EventKind::Trade(Trade {
                trade_id: 1,
                price: 1.0,
                qty: 1.0,
                taker_side: TradeSide::Buy,
            }),
        };
        // батч другой схемы — не наш файл
        let foreign = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("pair", DataType::Utf8, false)])),
            vec![Arc::new(StringArray::from(vec!["BTCUSDT"]))],
        )
        .unwrap();
        assert!(matches!(
            batch_to_events(&foreign),
            Err(crate::StoreError::Schema(_))
        ));

        // строка с неизвестным видом события
        let mut cols: Vec<ArrayRef> = event_to_batch(&event, Exchange::Binance)
            .unwrap()
            .columns()
            .to_vec();
        cols[0] = Arc::new(StringArray::from(vec!["???"]));
        let unknown_kind = RecordBatch::try_new(schema(), cols).unwrap();
        assert!(matches!(
            batch_to_events(&unknown_kind),
            Err(crate::StoreError::Schema(_))
        ));

        // сделка без идентификатора: payload пуст при виде «trade»
        let mut cols: Vec<ArrayRef> = event_to_batch(&event, Exchange::Bybit)
            .unwrap()
            .columns()
            .to_vec();
        let idx = schema().index_of("trade_id").unwrap();
        cols[idx] = Arc::new(Int64Array::from(Vec::<Option<i64>>::from([None])));
        let missing_payload = RecordBatch::try_new(schema(), cols).unwrap();
        assert!(matches!(
            batch_to_events(&missing_payload),
            Err(crate::StoreError::Schema(_))
        ));
    }

    /// Проверка схемы различает не только имена: та же схема по именам,
    /// но с nullable-парой — чужой контракт и отвергается.
    #[test]
    fn ensure_schema_rejects_nullability_drift() {
        assert!(ensure_schema(&schema()).is_ok());
        let drifted = Schema::new(
            schema()
                .fields()
                .iter()
                .map(|f| {
                    if f.name() == "pair" {
                        Field::new("pair", DataType::Utf8, true)
                    } else {
                        f.as_ref().clone()
                    }
                })
                .collect::<Vec<_>>(),
        );
        assert!(matches!(
            ensure_schema(&SchemaRef::new(drifted)),
            Err(crate::StoreError::Schema(_))
        ));
    }
}
