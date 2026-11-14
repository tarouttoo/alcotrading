//! Книга на 20 уровней на сторону: применение снимков и дельт, инварианты,
//! обрезка и флаг валидности.

/// Глубина книги: уровней на сторону. Спека фазы 1 пишет стакан 20 уровней
/// (канал depth20 у Binance, `book_depth = 20` в конфиге); константа одна на
/// весь конвейер — от ядра до схемы Parquet.
pub const BOOK_DEPTH: usize = 20;

/// Уровень стакана: цена и абсолютное количество (количество 0 удаляет
/// уровень — так отдают все три биржи).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    pub price: f64,
    pub qty: f64,
}

impl Level {
    pub const fn new(price: f64, qty: f64) -> Self {
        Self { price, qty }
    }
}

/// Состояние книги: заявки по убыванию цены, запросы по возрастанию, не более
/// [`BOOK_DEPTH`] уровней на сторону. Книга невалидна до первого снимка и
/// после разрыва — до следующего снимка от биржи; стратегия и replay это
/// видят (спека #13).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Book {
    bids: Vec<Level>,
    asks: Vec<Level>,
    valid: bool,
}

impl Book {
    /// Пустая невалидная книга.
    pub fn new() -> Self {
        Self::default()
    }

    /// Заявки (bids), по убыванию цены, топ-20.
    pub fn bids(&self) -> &[Level] {
        &self.bids
    }

    /// Запросы (asks), по возрастанию цены, топ-20.
    pub fn asks(&self) -> &[Level] {
        &self.asks
    }

    /// Валидность книги: применён ли снимок и не открыт ли разрыв.
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// Применить снимок: полная замена обеих сторон; книга становится
    /// валидной. Дубли цен в снимке схлопываются (последнее значение
    /// побеждает), некорректные уровни (NaN, отрицательное количество)
    /// отбрасываются.
    pub fn snapshot(&mut self, bids: Vec<Level>, asks: Vec<Level>) {
        let mut fresh_bids = Vec::new();
        merge_side(&mut fresh_bids, bids, Descending);
        let mut fresh_asks = Vec::new();
        merge_side(&mut fresh_asks, asks, Ascending);
        self.bids = fresh_bids;
        self.asks = fresh_asks;
        self.valid = true;
    }

    /// Применить дельту: абсолютные количества по ценам (0 — удалить уровень;
    /// удаление отсутствующего — норма). Возвращает `true`, если видимые
    /// уровни изменились. Валидность не меняет.
    pub fn delta(&mut self, bids: Vec<Level>, asks: Vec<Level>) -> bool {
        let (bids_before, asks_before) = (self.bids.clone(), self.asks.clone());
        merge_side(&mut self.bids, bids, Descending);
        merge_side(&mut self.asks, asks, Ascending);
        self.bids != bids_before || self.asks != asks_before
    }

    /// Пометить книгу невалидной (разрыв или переподключение): содержимое
    /// устарело, применять дельты нельзя до следующего снимка.
    pub fn invalidate(&mut self) {
        self.valid = false;
    }
}

/// Порядок сортировки стороны: заявки — по убыванию цены (лучшие — сверху),
/// запросы — по возрастанию.
#[derive(Clone, Copy)]
enum SideOrder {
    Descending,
    Ascending,
}
use SideOrder::{Ascending, Descending};

/// Слить обновления в сторону книги: количества абсолютные, 0 удаляет
/// уровень, удаление отсутствующего игнорируется; некорректные уровни
/// (не-конечная цена/количество, отрицательное количество) отбрасываются.
/// Сторона остаётся отсортированной по цене и обрезается до [`BOOK_DEPTH`].
fn merge_side(side: &mut Vec<Level>, updates: Vec<Level>, order: SideOrder) {
    for level in updates {
        if !level.price.is_finite() || !level.qty.is_finite() || level.qty < 0.0 {
            continue;
        }
        let found = match order {
            // заявки хранятся по убыванию цены, поэтому сравнение перевёрнуто
            Descending => side.binary_search_by(|l| level.price.total_cmp(&l.price)),
            Ascending => side.binary_search_by(|l| l.price.total_cmp(&level.price)),
        };
        match found {
            Ok(i) if level.qty == 0.0 => {
                side.remove(i);
            }
            Ok(i) => side[i] = level,
            Err(i) if level.qty != 0.0 => side.insert(i, level),
            Err(_) => {}
        }
    }
    side.truncate(BOOK_DEPTH);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels(prices: &[f64], qty: f64) -> Vec<Level> {
        prices.iter().map(|p| Level::new(*p, qty)).collect()
    }

    #[test]
    fn fresh_book_is_empty_and_invalid() {
        let book = Book::new();
        assert!(book.bids().is_empty());
        assert!(book.asks().is_empty());
        assert!(!book.is_valid());
    }

    #[test]
    fn snapshot_replaces_sides_sorts_and_marks_valid() {
        let mut book = Book::new();
        // намеренно вразброс: порядок сторон — инвариант книги
        book.snapshot(
            levels(&[99.5, 100.0, 99.0], 1.0),
            levels(&[101.0, 100.5, 101.5], 1.0),
        );
        assert!(book.is_valid());
        assert_eq!(book.bids(), levels(&[100.0, 99.5, 99.0], 1.0).as_slice());
        assert_eq!(book.asks(), levels(&[100.5, 101.0, 101.5], 1.0).as_slice());
    }

    #[test]
    fn snapshot_trims_to_twenty_levels_per_side() {
        let mut book = Book::new();
        let bid_prices: Vec<f64> = (0..30).map(|i| 100.0 - f64::from(i)).collect();
        let ask_prices: Vec<f64> = (0..30).map(|i| 100.0 + f64::from(i)).collect();
        book.snapshot(levels(&bid_prices, 1.0), levels(&ask_prices, 1.0));
        assert_eq!(book.bids().len(), BOOK_DEPTH);
        assert_eq!(book.asks().len(), BOOK_DEPTH);
        // у bids остались лучшие (наибольшие) цены, у asks — лучшие (наименьшие)
        assert_eq!(book.bids()[0].price, 100.0);
        assert_eq!(book.bids()[19].price, 81.0);
        assert_eq!(book.asks()[0].price, 100.0);
        assert_eq!(book.asks()[19].price, 119.0);
    }

    #[test]
    fn delta_overwrites_level_with_absolute_quantity() {
        let mut book = Book::new();
        book.snapshot(levels(&[100.0, 99.0], 1.0), levels(&[101.0], 1.0));
        let changed = book.delta(levels(&[100.0], 5.0), vec![]);
        assert!(changed);
        assert_eq!(book.bids(), [Level::new(100.0, 5.0), Level::new(99.0, 1.0)]);
    }

    #[test]
    fn delta_zero_quantity_removes_level() {
        let mut book = Book::new();
        book.snapshot(levels(&[100.0, 99.0], 1.0), levels(&[101.0], 1.0));
        let changed = book.delta(levels(&[99.0], 0.0), vec![]);
        assert!(changed);
        assert_eq!(book.bids(), levels(&[100.0], 1.0).as_slice());
    }

    #[test]
    fn delta_removing_absent_level_changes_nothing() {
        let mut book = Book::new();
        book.snapshot(levels(&[100.0], 1.0), levels(&[101.0], 1.0));
        let changed = book.delta(levels(&[90.0], 0.0), levels(&[120.0], 0.0));
        assert!(!changed);
        assert_eq!(book.bids(), levels(&[100.0], 1.0).as_slice());
        assert_eq!(book.asks(), levels(&[101.0], 1.0).as_slice());
    }

    #[test]
    fn delta_beyond_trim_window_keeps_top_twenty() {
        let mut book = Book::new();
        let bid_prices: Vec<f64> = (0..20).map(|i| 100.0 - f64::from(i)).collect();
        book.snapshot(levels(&bid_prices, 1.0), vec![]);
        // уровень хуже худшего видимого: книга остаётся топ-20 без изменений
        let changed = book.delta(levels(&[70.0], 9.0), vec![]);
        assert!(!changed);
        assert_eq!(book.bids().len(), BOOK_DEPTH);
        assert_eq!(book.bids()[19].price, 81.0);
    }

    #[test]
    fn delta_inserting_better_level_evicts_worst_of_full_book() {
        let mut book = Book::new();
        let bid_prices: Vec<f64> = (0..20).map(|i| 100.0 - f64::from(i)).collect();
        book.snapshot(levels(&bid_prices, 1.0), vec![]);
        // новая заявка лучше верхней: книга остаётся топ-20, худший уровень
        // (81.0) вытесняется — то, что видит рекордер на полном стакане
        let changed = book.delta(levels(&[101.0], 2.0), vec![]);
        assert!(changed);
        assert_eq!(book.bids().len(), BOOK_DEPTH);
        assert_eq!(book.bids()[0], Level::new(101.0, 2.0));
        assert_eq!(book.bids()[19].price, 82.0);
    }

    #[test]
    fn delta_does_not_flip_validity() {
        let mut book = Book::new();
        // дельта по ещё не синхронизированной книге не делает её валидной
        book.delta(levels(&[100.0], 1.0), vec![]);
        assert!(!book.is_valid());
        book.snapshot(levels(&[100.0], 1.0), levels(&[101.0], 1.0));
        book.delta(levels(&[100.0], 2.0), vec![]);
        assert!(book.is_valid());
    }

    #[test]
    fn invalidate_marks_stale_book() {
        let mut book = Book::new();
        book.snapshot(levels(&[100.0], 1.0), levels(&[101.0], 1.0));
        assert!(book.is_valid());
        book.invalidate();
        assert!(!book.is_valid());
        // содержимое остаётся для мониторинга, но помечено устаревшим
        assert_eq!(book.bids(), levels(&[100.0], 1.0).as_slice());
    }

    #[test]
    fn snapshot_dedupes_prices_last_value_wins_and_drops_malformed() {
        let mut book = Book::new();
        book.snapshot(
            vec![
                Level::new(100.0, 1.0),
                Level::new(100.0, 7.0),    // дубли цены: побеждает последнее
                Level::new(f64::NAN, 1.0), // некорректная цена — долой
                Level::new(99.0, -1.0),    // отрицательное количество — долой
                Level::new(98.0, f64::INFINITY), // некорректное количество — долой
            ],
            vec![],
        );
        assert_eq!(book.bids(), [Level::new(100.0, 7.0)]);
    }
}
