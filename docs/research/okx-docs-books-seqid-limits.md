# Проверка: официальная документация OKX (books, seqId, лимиты)

Исследование по тикету tarouttoo/alcotrading#3. Все факты ниже проверены по первоисточникам — официальной документации OKX API v5 (загружено `https://www.okx.com/docs-v5/en/`) и официальному changelog (`https://www.okx.com/docs-v5/log_en/`). Источник указан для каждого факта; сторонних источников не потребовалось.

## 1. Канал `books`: механика снимок/дельты — ОФИЦИАЛЬНО

Источник: OKX API v5 docs, раздел «WS / Order book channel» (<https://www.okx.com/docs-v5/en/#order-book-channel>).

- `books` — 400 уровней глубины. Первый пуш: полный снимок (`action: "snapshot"`), далее инкрементальные обновления (`action: "update"`) каждые **100 мс** с изменениями за этот период.
- `books5` / `bbo-tbt` / `bbo-rpi-tbt` — только снимки (100 мс / 10 мс / 10 мс при изменениях).
- `books-l2-tbt` (400 ур., 10 мс) и `books50-l2-tbt` (50 ур., 10 мс) — tick-by-tick, доступны только VIP4+ (иначе ошибка 64003).
- Нельзя одновременно подписываться на `books-l2-tbt` и `books50-l2-tbt`/`books` по одному инструменту (changelog 2024-07-17).
- Порядок пушей внутри одного соединения фиксирован: `bbo-tbt -> bbo-rpi-tbt -> books-l2-tbt -> books50-l2-tbt -> books -> books-rpi -> books5`.

## 2. `seqId` / `prevSeqId` — ОФИЦИАЛЬНО

Источник: OKX API v5 docs, подраздел «Sequence ID» того же раздела.

- `seqId` — порядковый номер публикуемых рыночных данных. Множество `seqId` одинаково для всех WebSocket-соединений на одном канале; у каждого `instId` свой уникальный ряд.
- `prevSeqId` нового сообщения совпадает с `seqId` предыдущего; обычно `seqId > prevSeqId`. Минимальное значение 0; в снимке `prevSeqId` всегда `-1`.
- Исключения (важно для кода):
  1. Нет изменений глубины ~60 с → для инкрементальных каналов OKX шлёт пустое сообщение `'asks': [], 'bids': []` (keep-alive), при этом `prevSeqId == seqId` (равны последнему отправленному).
  2. Сброс нумерации из-за обслуживания: приходит инкремент с `seqId < prevSeqId`; последующие сообщения снова идут по обычному правилу.
- Официальный пример: снимок `prevSeqId=-1, seqId=10`; инкремент 1 `10→15`; «нет обновлений» `15→15`; сброс `15→3`; далее `3→5`.
- Проверка целостности: сверять `prevSeqId` входящего с последним обработанным `seqId`; при разрыве — переподписка/запрос нового снимка. `prevSeqId` применим к `books`, `books-rpi`, `books-l2-tbt`, `books50-l2-tbt`.

## 3. Отказ от checksum — ОФИЦИАЛЬНО, дата: 2026-06-23

Источник: официальный changelog OKX, запись «2026-06-23 Deprecation of checksum field in order book channels» (<https://www.okx.com/docs-v5/log_en/>).

- В каналах `books`, `books-l2-tbt`, `books50-l2-tbt` поле `checksum` объявлено deprecated и в снимке, и в инкрементах: поле остаётся в пуше, но значение зафиксировано `0` и больше **нельзя** использовать для проверки целостности. Вместо него — `seqId`/`prevSeqId`.
- Примечания OKX из той же записи: `books5` и `bbo-tbt` поля checksum не имели и не затронуты; WebSocket принудительно на TLS (wss://), что вместе с валидацией `seqId`/`prevSeqId` даёт защиту, эквивалентную или сильнее прежнего checksum.
- (Справочно, там же: канал `books-rpi` изначально без checksum, секвенирование через `seqId`/`prevSeqId`; новый канал спредов `sprd-books-l2-tbt` — checksum сохранён.)

## 4. Лимиты WebSocket-подключений — ОФИЦИАЛЬНО

Источник: OKX API v5 docs, раздел «WebSocket Overview» (<https://www.okx.com/docs-v5/en/#websocket-overview>).

- **Connection limit: 3 запроса в секунду (по IP)** — да, подтверждено дословно: «Connection limit : 3 requests per second (based on IP)». Это лимит на установку соединений, не на подписки.
- **Request limit: суммарно 480 запросов `subscribe`/`unsubscribe`/`login` на соединение в час** — да, подтверждено дословно.
- Соединение рвётся, если подписка не установлена или данные не пушатся более **30 секунд** (рекомендуется ping/pong с таймером N < 30).
- Connection count limit: до **30 соединений на отдельный приватный канал (orders, account, positions, balance-and-positions, positions-risk, account-greeks) на субаккаунт**; на публичные рыночные каналы типа `books` этот лимит не распространяется.

## Сводка «официально/стороннее»

| Факт | Статус | Источник |
|---|---|---|
| `books`: снимок 400 ур. + инкременты 100 мс | Официально | docs-v5/en, Order book channel |
| `seqId`/`prevSeqId`, исключения (keep-alive, сброс) | Официально | docs-v5/en, Sequence ID |
| Checksum deprecated (значение 0), использовать seqId/prevSeqId | Официально | changelog 2026-06-23, docs-v5/en |
| 3 подключения/сек с IP | Официально | docs-v5/en, WebSocket Overview |
| 480 subscribe/unsubscribe/login в час на соединение | Официально | docs-v5/en, WebSocket Overview |
| 30 с бездействия → разрыв; ping/pong | Официально | docs-v5/en, WebSocket Overview |

Сторонние источники не использовались; все утверждения взяты напрямую с okx.com.
