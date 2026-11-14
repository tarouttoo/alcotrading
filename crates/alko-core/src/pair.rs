//! Канонический идентификатор пары и фиксированный список фазы 1 (ADR 0004).

use std::fmt;

/// Канонический идентификатор пары: `BASEUSDT` (глоссарий). Список пар
/// фазы 1 зафиксирован ADR 0004 на дату решения без механизма пересмотра —
/// пятёрка в [`Pair::ALL`] исчерпывающая, сконструировать пару извне нельзя.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pair(&'static str);

impl Pair {
    pub const BTCUSDT: Self = Self("BTCUSDT");
    pub const ETHUSDT: Self = Self("ETHUSDT");
    pub const SOLUSDT: Self = Self("SOLUSDT");
    pub const ZECUSDT: Self = Self("ZECUSDT");
    pub const XRPUSDT: Self = Self("XRPUSDT");

    /// Все пары фазы 1: якоря BTC, ETH, SOL и два «объёмных» слота ZEC, XRP
    /// (ADR 0004: только криптопары, минимальный суточный объём ≥ $100M на
    /// каждой из трёх бирж). Замена пары рвёт непрерывность данных.
    pub const ALL: [Self; 5] = [
        Self::BTCUSDT,
        Self::ETHUSDT,
        Self::SOLUSDT,
        Self::ZECUSDT,
        Self::XRPUSDT,
    ];

    /// Каноническая строка `BASEUSDT` — идентификатор записи и конфига.
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for Pair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ADR 0004 фиксирует ровно пять криптопар без механизма пересмотра.
    #[test]
    fn pair_list_matches_adr_0004() {
        assert_eq!(
            Pair::ALL,
            [
                Pair::BTCUSDT,
                Pair::ETHUSDT,
                Pair::SOLUSDT,
                Pair::ZECUSDT,
                Pair::XRPUSDT,
            ]
        );
        assert_eq!(Pair::BTCUSDT.as_str(), "BTCUSDT");
        assert_eq!(Pair::XRPUSDT.to_string(), "XRPUSDT");
    }
}
