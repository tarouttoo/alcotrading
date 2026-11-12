# Проверка: официальная документация Binance USDⓈ-M (фьючерсы)

Исследовательский тикет: tarouttoo/alcotrading#2. Все факты ниже помечены источником.
Обозначения: **[официально]** — первоисточник developers.binance.com (документация Binance);
**[стороннее]** — сторонние источники (в этом отчёте не используются для проверочных фактов).

Дата снятия данных: 2026-11-12 (пометка «Last modified on November 12, 2026» на страницах docs).

## 1. Лимиты потоков на соединение

**[официально]** — Websocket Market Streams → Connect:
https://developers.binance.com/docs/derivatives/usds-margined-futures/websocket-market-streams/Connect

- Одно соединение может слушать максимум **1024 стрима**.
- Лимит **10 входящих сообщений в секунду** на WS-соединение (SUBSCRIBE/UNSUBSCRIBE и т.п.).
- Превышение лимита → разрыв соединения; повторно отключаемые IP могут быть забанены.
- Базовый URL: `wss://fstream.binance.com`; три маршрутизированных эндпоинта:
  `/public` (высокочастотные публичные данные, в т.ч. `@depth`), `/market` (обычные рыночные данные, в т.ч. `@markPrice`), `/private` (user data).
  Соединение без маршрута (`/ws/...`) получает данные только `/public`.
- Два режима доступа: `ws` mode (`/ws/<streamName>`) и `stream` mode (`/stream?streams=a/b/c`).
- Комбинированные события оборачиваются: `{"stream":"<streamName>","data":<rawPayload>}`.
- Все символы в именах стримов — lowercase.

## 2. Ping/pong политика

**[официально]** — там же (Connect):

- Сервер отправляет **ping frame каждые 3 минуты**.
- Если сервер не получит `pong frame` в течение **10 минут** — соединение разрывается.
- Непроcимые (unsolicited) `pong frame` от клиента разрешены: клиент может слать pong чаще,
  чем раз в 15 минут, для поддержания соединения.

## 3. Поле `pu` и правила сопоставления снимок/дельта (futures)

**[официально]** — «How to manage a local order book correctly»:
https://developers.binance.com/docs/derivatives/usds-margined-futures/websocket-market-streams/How-to-manage-a-local-order-book-correctly

Официальный алгоритм поддержания локального стакана:

1. Открыть стрим `wss://fstream.binance.com/public/stream?streams=btcusdt@depth` (буферизовать события).
2. Для одной и той же цены последнее полученное обновление перекрывает предыдущее.
3. Взять REST-снимок: `GET https://fapi.binance.com/fapi/v1/depth?symbol=BTCUSDT&limit=1000`.
4. Отбросить события, где `u` < `lastUpdateId` снимка.
5. Первое обрабатываемое событие должно удовлетворять: `U` <= `lastUpdateId` **И** `u` >= `lastUpdateId`,
   где `U` = firstUpdateId, `u` = finalUpdateId, `lastUpdateId` = из REST-снимка.
6. Далее: **`pu` каждого нового события должен равняться `u` предыдущего события**;
   при несовпадении — инициализировать процесс заново с шага 3 (новый снимок + синхронизация).
7. Количества в событии — **абсолютные** (не дельты): нужно перезаписывать уровень целиком.
8. Количество 0 → уровень **удаляется**.
9. Удаление отсутствующего в локальной книге уровня — нормальная ситуация.

То есть ключевое отличие futures от spot: непрерывность проверяется цепочкой `pu == prev(u)`
(на споте вместо этого диапазоны `U`/`u` перекрываются между событиями).

Схема payload diff-стрима (для `@depth@100ms`) содержит поля `e, E, T, s, U, u, pu, b, a` — **[официально]**,
каталог WebSocket Streams:
https://developers.binance.com/en/docs/catalog/core-trading-derivatives-trading-usd-s-m-futures/api/ws-streams/public
После «CM migration» payload дополнен полями `st` (1 = UM, 2 = CM) и `ps` (pair symbol).

## 4. Канал на 20 уровней vs полная книга

**[официально]** — каталог WebSocket Streams (там же):

- **Partial Book Depth Streams**: `{symbol}@depth{levels}@{updateSpeed}` — топ-N уровней bids/asks,
  `levels` ∈ {5, 10, 20}; `updateSpeed` ∈ {100ms, 500ms} (в перечислении параметров; текст страницы
  называет также 250ms). Пример: `btcusdt@depth20@100ms`.
  Это уже готовые снимки топа книги: поддержание локального стакана не требуется.
  Payload содержит `e, E, T, s, U, u, pu, b, a` (согласно каталогу), где `pu` = final update Id
  в последнем стриме (т.е. «Final update Id in last stream (ie `u` in last stream)»).
- **Diff. Book Depth Streams**: `{symbol}@depth@{updateSpeed}`, updateSpeed ∈ {100ms, 500ms} —
  дифференциальные обновления полной книги, требуют локальной книги по алгоритму раздела 3.
- **RPI Diff. Book Depth Streams**: `{symbol}@rpiDepth@500ms` — вариант для ребалансировки индекса.
- Полная глубина по REST: `GET /fapi/v1/depth?limit=1000` (максимум 1000 уровней) — **[официально]**,
  страница «How to manage…» (см. раздел 3).

Вывод для очереди проверок: канал на 20 уровней существует официально
(`{symbol}@depth20@100ms`) и не требует восстановления книги снимок+дельта; альтернатива —
`@depth@100ms` + локальная книга на правилах `pu`.

## 5. 24-часовой жизненный цикл соединения

**[официально]** — Connect (раздел 1):

- Соединение действует **максимум 24 часа**; следует ожидать отключения на 24-часовой отметке.
  Клиент обязан предусмотреть реконнект (и повторную синхронизацию книги после него).

## 6. Hedge mode (контекст для фазы 2)

**[официально]** — каталог REST API USDⓈ-M:
https://developers.binance.com/en/docs/catalog/core-trading-derivatives-trading-usd-s-m-futures/api/rest-api

- `GET /fapi/v1/positionSide/dual` (Get Current Position Mode, USER_DATA): возвращает
  `dualSidePosition` — режим позиции на **всех символах сразу**: `true` = Hedge Mode, `false` = One-way Mode.
- `POST /fapi/v1/positionSide/dual` (Change Position Mode, TRADE, подпись, вес IP = 1):
  параметр `dualSidePosition` (`"true"`/`"false"`), меняет режим на **EVERY symbol**.
  После CM-миграции UM и CM делят одну настройку `dualSidePosition`: вызов переключает обе стороны сразу.
  Смена **отклоняется**, если есть открытые ордера (код `-4067`) или открытые позиции (`-4068`).
- В ордерах (`New Order` и др.) поле `positionSide` принимает значения `BOTH` (one-way) /
  `LONG`/`SHORT` (hedge); в примерах ответов фигурируют `"positionSide": "SHORT"` и `"BOTH"`.
- Position Information: one-way возвращает `BOTH`, hedge — `LONG`/`SHORT` (примечание в каталоге).

## Итог по пунктам очереди проверок

| Пункт проверки | Ответ (источник: официальная документация Binance) |
|---|---|
| Лимит стримов на соединение | 1024 стрима; 10 входящих сообщений/сек; превышение → разрыв, риск бана IP |
| Ping-политика | ping от сервера каждые 3 мин; pong обязателен в течение 10 мин; unsolicited pong разрешён (чаще раза в 15 мин) |
| `pu` и снимок/дельта (futures) | снимок `/fapi/v1/depth?limit=1000`; первое событие `U<=lastUpdateId && u>=lastUpdateId`; далее `pu == prev(u)`, иначе ресинхронизация; количества абсолютные; 0 → удаление уровня |
| Канал на 20 уровней | официальный partial book depth stream `{symbol}@depth20@100ms` (levels 5/10/20; скорости 100ms/500ms), без поддержки локальной книги |
| 24-часовой разрыв | соединение валидно максимум 24 часа, дисконнект на отметке 24 ч обязателен к обработке |
| Hedge mode (фаза 2) | `dualSidePosition` глобально на все символы; смена через POST `/fapi/v1/positionSide/dual` запрещена при открытых ордерах/позициях; positionSide BOTH vs LONG/SHORT |

Сторонние источники для проверочных фактов не использовались.
