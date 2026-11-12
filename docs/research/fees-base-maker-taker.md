# Проверка: базовые тарифы maker/taker трёх бирж (тикет карты «Проверка: базовые тарифы maker/taker трёх бирж», Q52)

Дата проверки: 2026-02-13. Все значения — **базовый (не-VIP) уровень** на официальных
страницах бирж, для бессрочных фьючерсов (perpetual). Формат — «maker/taker», в % и bps,
готово для подстановки в конфиг playground/paper.

## Сводка для конфига

| Биржа | Продукт | Maker | Taker |
|---|---|---|---|
| Binance | USDⓈ-M futures (USDT-пар) | 0.0200% = 2.0 bps | 0.0500% = 5.0 bps |
| Bybit | Derivatives (USDT/inverse perps) | 0.0200% = 2.0 bps | 0.0550% = 5.5 bps |
| OKX | Futures / Swap (USDT-M и crypto-M) | 0.0200% = 2.0 bps | 0.0500% = 5.0 bps |

В виде доли (для конфига):

```yaml
fees:
  binance_usdm: { maker: 0.0002, taker: 0.0005 }   # 2 bps / 5 bps
  bybit_deriv:  { maker: 0.0002, taker: 0.00055 }  # 2 bps / 5.5 bps
  okx_futures:  { maker: 0.0002, taker: 0.0005 }   # 2 bps / 5 bps
```

## Источники (официальные страницы)

### Binance USDⓈ-M futures
- Страница: https://www.binance.com/en/fee/futureFee (вкладка USDⓈ-M Futures).
- Строка «Regular User» (< 5,000,000 USD 30D объёма или ≥ 0 BNB):
  **USDT Maker/Taker = 0.0200% / 0.0500%**.
- Примечание: для USDC/USD1-пар сейчас действует промо (0.0000% maker / 0.0400% taker
  для Regular User). Это временная акция, в базовый конфиг не брать; базовые USDT-тарифы
  — 2/5 bps.

### Bybit derivatives
- Статья хелп-центра «Bybit Trading Fee Structure»:
  https://www.bybit.com/en/help-center/article/Trading-Fee-Structure
  (страница тарифов: https://www.bybit.com/en/fees-and-limits/futures/)
- Таблица «Trading Fee Rate», строка **VIP 0** (базовый уровень), колонка
  «Perpetual & Futures Contracts Trading»: **Taker 0.0550% / Maker 0.0200%**.
- Bybit предупреждает, что фактические ставки могут отличаться по региону — точное
  значение для аккаунта видно на https://www.bybit.com/en/my-fee-rate после KYC.

### OKX futures/swap
- Страница тарифов: https://www.okx.com/fees (вкладка Trading fees → Futures).
- Таблицы тиров на странице рендерятся клиентским JS, но в правилах расчёта комиссий
  на самой официальной странице для бессрочных фьючерсов обоих типов приведены примеры
  «Trader A (Maker fee: 0.02%; Taker fee: 0.05%)»:
  - USDT-M perpetual: https://www.okx.com/fees (раздел «Trading fee rules → USDT-M perpetual and expiry»);
  - crypto-M perpetual: там же, «Crypto-M perpetual and expiry».
- Это стандартный Regular-user тир OKX для фьючерсов: **maker 0.0200% / taker 0.0500%**.
- Верификация под конкретный аккаунт: приватный эндпоинт API
  `GET /api/v5/account/trade-fee?instType=SWAP` (https://www.okx.com/docs-v5/en/) —
  возвращает takerFee/makerFee аккаунта.

## Замечания

- Все три биржи: базовый maker одинаковый (2 bps); taker — 5 bps (Binance, OKX) и
  5.5 bps (Bybit).
- Ставки меняются (промо Binance USDC, региональные отличия Bybit) — при заведении
  реальных комиссий в конфиг фазы 1 свериться повторно; для playground/paper достаточно
  приведённых базовых значений.
- BNB-скидка 10% и прочие опции не учитывались (базовые ставки без скидок).
