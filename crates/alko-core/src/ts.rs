//! Метки времени PERD одной записи (ADR 0005).

/// Четыре метки времени одной записи (ADR 0005): nullable int64 микросекунды
/// UTC. `None` — честный пропуск: биржа не различает метку (например, у книги
/// Binance нет отдельного пуша) или потребитель ещё не проставил свою
/// (`delivered` ставит очередь потребителя, не коннектор).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timestamps {
    /// `tsEvent` — время события у матчинга биржи.
    pub event: Option<i64>,
    /// `tsPush` — время пуша биржи; `None`, где биржа не различает пуш.
    pub push: Option<i64>,
    /// `tsReceived` — локальный приём WS-фрейма, до парсинга.
    pub received: Option<i64>,
    /// `tsDelivered` — принятие в очередь потребителя (буфер писателя Parquet
    /// в фазе 1, вход в ядро стратегии в live); проставляется потребителем.
    pub delivered: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Незаполненные метки — `None` по всем четырём колонкам (ADR 0005:
    /// nullable, честный пропуск вместо дублирования).
    #[test]
    fn default_timestamps_are_all_none() {
        let ts = Timestamps::default();
        assert_eq!(ts.event, None);
        assert_eq!(ts.push, None);
        assert_eq!(ts.received, None);
        assert_eq!(ts.delivered, None);
    }
}
