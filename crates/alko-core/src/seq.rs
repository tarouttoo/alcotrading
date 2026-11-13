//! Нативные последовательности обновлений книги: нормализованные метки и
//! проверка непрерывности по схемам трёх бирж.

/// Вид обновления книги в потоке: снимок (полное состояние) или дельта
/// (абсолютные количества по изменившимся ценам).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BookInputKind {
    Snapshot,
    Delta,
}

/// Схема нативной последовательности, фиксируется на поток книги.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeqScheme {
    /// Binance USDⓈ-M: непрерывность по цепочке `pu == u` предыдущего события
    /// (официальный алгоритм «How to manage a local order book correctly»).
    /// Несовпадение — ресинк со снимка, не ошибка. Checksum не используется.
    Binance,
    /// Bybit derivatives: монотонный идентификатор обновления `u`; `u == 1` —
    /// полный сброс книги (снимок после переподключения).
    Bybit,
    /// OKX: `seqId`/`prevSeqId`; у снимка `prevSeqId == -1`; keep-alive без
    /// изменений глубины шлёт `seqId == prevSeqId`; сброс нумерации при
    /// обслуживании — инкремент с `seqId < prevSeqId`, данные непрерывны
    /// (официальный пример docs-v5: 10 → 15 → 15 → 3 → 5).
    Okx,
}

/// Нормализованная метка последовательности одного обновления книги: числа
/// нативных идентификаторов биржи без её JSON — нормализация сырых кадров
/// задача коннектора. Идентификаторы int64 (отрицательное у меток — только
/// маркер снимка OKX `-1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BookSeq {
    /// Binance USDⓈ-M: `U` — первый и `u` — последний идентификаторы диапазона
    /// обновления, `pu` — `u` предыдущего события потока. У REST-снимка `pu`
    /// нет; коннектор передаёт `first == last == prev_last == lastUpdateId` —
    /// базовая линия ставится по `last` (фильтрация буфера `u < lastUpdateId`
    /// и правило первого события `U <= lastUpdateId <= u` — сторона
    /// коннектора, фаза 1 использует канал depth20 без REST-бутстрапа).
    Binance {
        first: i64,
        last: i64,
        prev_last: i64,
    },
    /// Bybit derivatives: идентификатор обновления `u`.
    Bybit { update_id: i64 },
    /// OKX: `seqId` и `prevSeqId` этого сообщения.
    Okx { seq_id: i64, prev_seq_id: i64 },
}

impl BookSeq {
    /// Собственный идентификатор обновления — им размечаются границы разрывов.
    pub fn own_id(&self) -> i64 {
        match self {
            BookSeq::Binance { last, .. } => *last,
            BookSeq::Bybit { update_id } => *update_id,
            BookSeq::Okx { seq_id, .. } => *seq_id,
        }
    }

    /// Схема, к которой относится метка.
    fn scheme(&self) -> SeqScheme {
        match self {
            BookSeq::Binance { .. } => SeqScheme::Binance,
            BookSeq::Bybit { .. } => SeqScheme::Bybit,
            BookSeq::Okx { .. } => SeqScheme::Okx,
        }
    }
}

/// Вердикт проверки последовательности входящего обновления книги.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SeqVerdict {
    /// Последовательность непрерывна: применить обновление.
    Continue,
    /// Применить, но последовательность прервана (снимок Binance с
    /// `pu !=` базовой линии): разрыв закрывается этим же снимком.
    ContinueWithGap,
    /// Последовательность прервана: не применять, книга невалидна до снимка.
    Gap,
    /// Нарушение протокола схемы в снимке (Bybit `u != 1`, OKX
    /// `prevSeqId != -1`, чужая метка): не применять.
    MalformedSnapshot,
}

/// Проверить непрерывность входящего обновления по схеме биржи против
/// базовой линии (идентификатор последнего применённого обновления;
/// `None`, пока снимок не приходил).
pub(crate) fn check_seq(
    scheme: SeqScheme,
    kind: BookInputKind,
    seq: BookSeq,
    baseline: Option<i64>,
) -> SeqVerdict {
    let rejected = match kind {
        BookInputKind::Snapshot => SeqVerdict::MalformedSnapshot,
        BookInputKind::Delta => SeqVerdict::Gap,
    };
    // чужая метка для схемы или отрицательный идентификатор — мусор коннектора
    if seq.scheme() != scheme || seq.own_id() < 0 {
        return rejected;
    }
    match (kind, seq, baseline) {
        // Binance: снимок применяется всегда (полное состояние), но разрыв
        // цепочки `pu` помечается — обновления потока пропущены
        (BookInputKind::Snapshot, BookSeq::Binance { prev_last, .. }, Some(b))
            if prev_last != b =>
        {
            SeqVerdict::ContinueWithGap
        }
        (BookInputKind::Snapshot, BookSeq::Binance { .. }, _) => SeqVerdict::Continue,
        (BookInputKind::Delta, BookSeq::Binance { prev_last, .. }, b) if Some(prev_last) == b => {
            SeqVerdict::Continue
        }
        (BookInputKind::Delta, BookSeq::Binance { .. }, _) => SeqVerdict::Gap,
        // Bybit: полный сброс книги строго `u == 1`
        (BookInputKind::Snapshot, BookSeq::Bybit { update_id: 1 }, _) => SeqVerdict::Continue,
        (BookInputKind::Snapshot, BookSeq::Bybit { .. }, _) => SeqVerdict::MalformedSnapshot,
        (BookInputKind::Delta, BookSeq::Bybit { update_id }, Some(b)) if update_id == b + 1 => {
            SeqVerdict::Continue
        }
        (BookInputKind::Delta, BookSeq::Bybit { .. }, _) => SeqVerdict::Gap,
        // OKX: в снимке `prevSeqId` всегда `-1`; keep-alive (`prevSeqId ==
        // seqId`) и сброс нумерации (`seqId < prevSeqId`) непрерывны, пока
        // `prevSeqId` сходится с базовой линией
        (
            BookInputKind::Snapshot,
            BookSeq::Okx {
                prev_seq_id: -1, ..
            },
            _,
        ) => SeqVerdict::Continue,
        (BookInputKind::Snapshot, BookSeq::Okx { .. }, _) => SeqVerdict::MalformedSnapshot,
        (BookInputKind::Delta, BookSeq::Okx { prev_seq_id, .. }, b) if Some(prev_seq_id) == b => {
            SeqVerdict::Continue
        }
        (BookInputKind::Delta, BookSeq::Okx { .. }, _) => SeqVerdict::Gap,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use BookInputKind::{Delta, Snapshot};

    #[test]
    fn own_id_returns_native_update_id() {
        assert_eq!(
            BookSeq::Binance {
                first: 10,
                last: 15,
                prev_last: 9
            }
            .own_id(),
            15
        );
        assert_eq!(BookSeq::Bybit { update_id: 7 }.own_id(), 7);
        assert_eq!(
            BookSeq::Okx {
                seq_id: 20,
                prev_seq_id: 19
            }
            .own_id(),
            20
        );
    }

    // Binance: цепочка `pu == u` предыдущего события

    #[test]
    fn binance_delta_with_matching_pu_is_valid() {
        let verdict = check_seq(
            SeqScheme::Binance,
            Delta,
            BookSeq::Binance {
                first: 1001,
                last: 1005,
                prev_last: 1000,
            },
            Some(1000),
        );
        assert_eq!(verdict, SeqVerdict::Continue);
    }

    #[test]
    fn binance_delta_with_wrong_pu_is_a_gap() {
        let verdict = check_seq(
            SeqScheme::Binance,
            Delta,
            BookSeq::Binance {
                first: 1001,
                last: 1005,
                prev_last: 1002,
            },
            Some(1000),
        );
        assert_eq!(verdict, SeqVerdict::Gap);
    }

    #[test]
    fn binance_delta_without_baseline_is_a_gap() {
        let verdict = check_seq(
            SeqScheme::Binance,
            Delta,
            BookSeq::Binance {
                first: 1,
                last: 5,
                prev_last: 0,
            },
            None,
        );
        assert_eq!(verdict, SeqVerdict::Gap);
    }

    #[test]
    fn binance_first_snapshot_is_valid() {
        let verdict = check_seq(
            SeqScheme::Binance,
            Snapshot,
            BookSeq::Binance {
                first: 1000,
                last: 1000,
                prev_last: 1000,
            },
            None,
        );
        assert_eq!(verdict, SeqVerdict::Continue);
    }

    #[test]
    fn binance_snapshot_with_wrong_pu_reports_gap_but_applies() {
        // снимок — полное состояние: разрыв помечается, но им же и закрывается
        let verdict = check_seq(
            SeqScheme::Binance,
            Snapshot,
            BookSeq::Binance {
                first: 5000,
                last: 5000,
                prev_last: 5049,
            },
            Some(1005),
        );
        assert_eq!(verdict, SeqVerdict::ContinueWithGap);
    }

    // Bybit: монотонный `u`, `u == 1` — полный сброс

    #[test]
    fn bybit_delta_with_next_update_id_is_valid() {
        let verdict = check_seq(
            SeqScheme::Bybit,
            Delta,
            BookSeq::Bybit { update_id: 3 },
            Some(2),
        );
        assert_eq!(verdict, SeqVerdict::Continue);
    }

    #[test]
    fn bybit_delta_with_skipped_update_id_is_a_gap() {
        let verdict = check_seq(
            SeqScheme::Bybit,
            Delta,
            BookSeq::Bybit { update_id: 9 },
            Some(2),
        );
        assert_eq!(verdict, SeqVerdict::Gap);
    }

    #[test]
    fn bybit_delta_with_stale_update_id_is_a_gap() {
        let verdict = check_seq(
            SeqScheme::Bybit,
            Delta,
            BookSeq::Bybit { update_id: 2 },
            Some(5),
        );
        assert_eq!(verdict, SeqVerdict::Gap);
    }

    #[test]
    fn bybit_snapshot_requires_u_equals_one() {
        let ok = check_seq(
            SeqScheme::Bybit,
            Snapshot,
            BookSeq::Bybit { update_id: 1 },
            Some(9),
        );
        assert_eq!(ok, SeqVerdict::Continue);
        let bad = check_seq(
            SeqScheme::Bybit,
            Snapshot,
            BookSeq::Bybit { update_id: 7 },
            None,
        );
        assert_eq!(bad, SeqVerdict::MalformedSnapshot);
    }

    // OKX: `seqId`/`prevSeqId`, снимок `prevSeqId == -1`

    #[test]
    fn okx_snapshot_requires_prev_seq_id_minus_one() {
        let ok = check_seq(
            SeqScheme::Okx,
            Snapshot,
            BookSeq::Okx {
                seq_id: 10,
                prev_seq_id: -1,
            },
            None,
        );
        assert_eq!(ok, SeqVerdict::Continue);
        let bad = check_seq(
            SeqScheme::Okx,
            Snapshot,
            BookSeq::Okx {
                seq_id: 10,
                prev_seq_id: 9,
            },
            None,
        );
        assert_eq!(bad, SeqVerdict::MalformedSnapshot);
    }

    #[test]
    fn okx_delta_with_matching_prev_is_valid() {
        let verdict = check_seq(
            SeqScheme::Okx,
            Delta,
            BookSeq::Okx {
                seq_id: 15,
                prev_seq_id: 10,
            },
            Some(10),
        );
        assert_eq!(verdict, SeqVerdict::Continue);
    }

    #[test]
    fn okx_keepalive_is_valid_and_continuous() {
        // keep-alive: пустые изменения, prevSeqId == seqId == последний посланный
        let verdict = check_seq(
            SeqScheme::Okx,
            Delta,
            BookSeq::Okx {
                seq_id: 15,
                prev_seq_id: 15,
            },
            Some(15),
        );
        assert_eq!(verdict, SeqVerdict::Continue);
    }

    #[test]
    fn okx_numbering_reset_is_valid_continuation() {
        // сброс нумерации из-за обслуживания (официальный пример: 15 → 3)
        let verdict = check_seq(
            SeqScheme::Okx,
            Delta,
            BookSeq::Okx {
                seq_id: 3,
                prev_seq_id: 15,
            },
            Some(15),
        );
        assert_eq!(verdict, SeqVerdict::Continue);
    }

    #[test]
    fn okx_delta_with_wrong_prev_is_a_gap() {
        let verdict = check_seq(
            SeqScheme::Okx,
            Delta,
            BookSeq::Okx {
                seq_id: 20,
                prev_seq_id: 17,
            },
            Some(15),
        );
        assert_eq!(verdict, SeqVerdict::Gap);
    }

    #[test]
    fn okx_delta_without_baseline_is_a_gap() {
        let verdict = check_seq(
            SeqScheme::Okx,
            Delta,
            BookSeq::Okx {
                seq_id: 5,
                prev_seq_id: 3,
            },
            None,
        );
        assert_eq!(verdict, SeqVerdict::Gap);
    }

    // Чужая метка для схемы — нарушение протокола

    #[test]
    fn foreign_seq_stamp_is_rejected() {
        let snapshot = check_seq(
            SeqScheme::Okx,
            Snapshot,
            BookSeq::Bybit { update_id: 1 },
            None,
        );
        assert_eq!(snapshot, SeqVerdict::MalformedSnapshot);
        let delta = check_seq(
            SeqScheme::Okx,
            Delta,
            BookSeq::Binance {
                first: 1,
                last: 2,
                prev_last: 1,
            },
            Some(1),
        );
        assert_eq!(delta, SeqVerdict::Gap);
    }

    // Отрицательные идентификаторы (кроме маркера снимка OKX) — мусор

    #[test]
    fn negative_ids_are_rejected() {
        let verdict = check_seq(
            SeqScheme::Bybit,
            Delta,
            BookSeq::Bybit { update_id: -5 },
            Some(-6),
        );
        assert_eq!(verdict, SeqVerdict::Gap);
    }
}
