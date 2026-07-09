# orderbook-tests

Integration tests for the Option Chain OrderBook API. They run against a
**running** server over HTTP/WS using the typed `orderbook-client` SDK.

## Prerequisites

The API server must be running, with JWT auth configured, before executing these
tests. Every endpoint except `/health` and `POST /api/v1/auth/token` requires a
Bearer token, which the tests mint using the operator bootstrap secret.

Start a local dev server:

```bash
ALLOW_DEV_KEY=1 AUTH_BOOTSTRAP_SECRET=test-bootstrap-secret cargo run
```

## Configuration

| Variable                | Default                  | Purpose                                   |
| ----------------------- | ------------------------ | ----------------------------------------- |
| `API_BASE_URL`          | `http://localhost:8080`  | Base URL of the running server.           |
| `AUTH_BOOTSTRAP_SECRET` | `test-bootstrap-secret`  | Operator secret used to mint test tokens. |

## Running

```bash
# Whole suite
cargo test -p orderbook-tests

# A single suite
cargo test -p orderbook-tests --test orders
cargo test -p orderbook-tests --test market_data

# With output
cargo test -p orderbook-tests -- --nocapture
```

Using Docker Compose:

```bash
docker compose --profile test up
```

## Test isolation, auth, and rate limits

- **Unique symbols.** Each test creates underlyings with a unique, timestamped
  prefix (`unique_symbol`) and deletes them best-effort at the end.
- **Shared cached tokens.** Issuing a token is itself rate-limited per client IP
  (10 / 60s), while each token's subject is limited to 100 requests / 60s. The
  helpers in `src/lib.rs` mint a small, cross-process-cached set of tokens (a
  pool of `Admin` subjects plus one `Read` subject) and reuse them across every
  test binary, keeping both limits satisfied for a single suite run. Running the
  *entire* suite several times within the same 60-second window can still trip
  the server's per-subject limiter — this is the server protection working as
  intended; allow the window to lapse (or run a subset) between rapid re-runs.
- **Expiration handling (server bug #110).** A numeric expiration segment such as
  `20251231` is parsed as a *number of days*, so a user-created expiration is
  stored under a server-formatted canonical string (e.g. `+574720704`). Order
  **placement** resolves the book by parsing the sent value, while the **read /
  modify / cancel / bulk** paths resolve it by the formatted string. The
  `setup_underlying` helper returns that formatted expiration; tests place with
  `TEST_EXPIRATION` and read/modify/cancel with the formatted value.
- **Global controls.** Tests that mutate global market-maker parameters or the
  kill switch restore the initial values before returning; the config-provisioned
  `BTC`/`ETH`/`GOLD` books' controls are never left mutated. See
  [Isolation & cleanup](#isolation--cleanup) for the mechanics.

## Isolation & cleanup

Two conventions keep the suite deterministic against a shared, stateful server —
both are enforced by the tests themselves, not by the test runner.

### The control lock

The `/api/v1/controls/*` endpoints act on **process-global** server state (the
master kill switch and the shared quoting parameters). Within a single test
binary `cargo test` runs the test functions concurrently on a thread pool, so two
control tests could otherwise interleave a read between another test's write and
its restore, or clobber each other's restore.

Every test that mutates (or reads under mutation of) global controls acquires the
shared `control_lock()` — a process-wide `tokio::sync::Mutex<()>` defined in
`src/lib.rs` — for the whole test body:

```rust
let _guard = control_lock().lock().await;
```

This is an **in-process** lock, so it only serializes tests *within one test
binary*. That is sufficient because `cargo test` runs the test binaries
themselves sequentially by default (it parallelizes test functions inside a
binary, not the binaries against one another), so no two binaries touch the
control endpoints at the same time. If that default ever changes (e.g.
`cargo nextest` running binaries in parallel), the lock would need to become a
cross-process guard.

### Capture-then-assert (panic-safe cleanup/restore)

Every test that (a) creates an underlying or (b) mutates global controls must
still delete the underlying / restore the state **even if an assertion fails
mid-test**. A panicking `assert!` would otherwise skip a trailing cleanup call and
leak state into the next test.

Because there is no dependency-free way to run cleanup on a future's panic
(`catch_unwind` needs `UnwindSafe` futures and would add a dependency), the tests
use the **capture-then-assert** pattern instead:

1. **Phase 1 — act.** Perform every request, capturing each outcome into a plain
   variable (a `Result`), with *no assertions interleaved*. Requests that depend
   on an earlier one thread the value through `Option`/`Result` (e.g. a cancel
   only runs if the placement returned an id).
2. **Phase 2 — clean up.** Run `cleanup_underlying(...)` and/or the control
   restore. These are best-effort and never assert.
3. **Phase 3 — assert.** Unwrap the captured values and assert on them.

Because cleanup runs before any assertion, a failed assertion in phase 3 still
leaves the server clean. Restores use the endpoints' absolute-set semantics
(`enable_quoting` / `kill_switch` set the master state outright; a parameter POST
sets the values outright), so a single unconditional call restores the initial
state even if a phase-1 mutation failed part-way.

## Test coverage

### `tests/health.rs`
- `test_health_check` — `/health` (unauthenticated) returns `healthy`.
- `test_global_stats` — `/api/v1/stats` with a Read token.

### `tests/auth.rs`
- `test_authorized_read_succeeds` — Read token reaches a read endpoint.
- `test_missing_token_is_unauthorized` — no token → 401.
- `test_invalid_token_is_unauthorized` — malformed token → 401.
- `test_insufficient_permission_is_forbidden` — Read token on controls → 403.
- `test_admin_token_reaches_controls` — Admin token reaches controls.
- `test_readonly_token_cannot_control_over_ws` — Read token `kill` over WS → error.

### `tests/orderbook.rs`
- `test_create_and_list_underlyings` — create / list / get / delete underlying.
- `test_create_expiration_and_strike` — create expiration + strike, list both.
- `test_add_and_cancel_order` — add a limit order, see it in the book, cancel it.
- `test_get_option_quote` — best bid/ask from resting orders.
- `test_market_order_execution` — market order crosses resting liquidity.
- `test_market_order_no_liquidity` — market order with an empty book errors.
- `test_put_option_operations` — put-side add + book read.

### `tests/orders.rs`
- `test_order_status_and_list` — `get_order_status` (flat) + `list_orders` filter.
- `test_modify_order` — cancel-and-replace modify; book reflects new price/size.
- `test_bulk_submit_partial` — non-atomic bulk: one accepted, one rejected.
- `test_bulk_submit_atomic_rollback` — atomic bulk rolls back on a failure.
- `test_bulk_cancel` — bulk cancel of known + unknown ids.
- `test_cancel_all` — cancel-all filtered by underlying.

### `tests/market_data.rs`
- `test_enriched_snapshot` — enriched snapshot with `depth` = `10` and `full`.
- `test_orderbook_metrics` — spread / depth / price metrics.
- `test_ohlc_from_fills` — OHLC bar OHLC/volume/trade-count from a fill.
- `test_last_trade` — last trade after a crossing fill.
- `test_option_chain` — chain row bid/ask.
- `test_volatility_surface` — surface shape (IVs may be null).
- `test_strike_and_expiration_details` — `get_expiration` + `get_strike`.
- `test_greeks_on_priced_underlying` — greeks against the priced `BTC` book.

### `tests/financial.rs`
- `test_priced_position_pnl` — position mark, unrealized PnL, notional, summary.
- `test_executions_list_and_get` — executions list + summary and fetch by id.

### `tests/positions.rs`
- `test_unpriced_position_omits_mark_fields` — unpriced position omits mark fields
  (issue #59).

### `tests/market_maker.rs`
- `test_get_controls` — control status ranges.
- `test_kill_switch_and_enable` — kill / enable master switch (state restored).
- `test_update_parameters` — parameter round-trip (issue #82; values restored).
- `test_list_instruments` — a created underlying appears as an instrument.
- `test_toggle_instrument` — per-instrument quoting toggle.
- `test_insert_and_get_price` — insert an underlying price, read it back.
- `test_get_all_prices` — a price appears in the all-prices list.
- `test_price_not_found` — unknown symbol → 404.

### `tests/websocket.rs`
- `test_websocket_connection` — authenticated `/ws` upgrade + first message.
- `test_websocket_subscribe` — subscribe / unsubscribe commands.
- `test_websocket_heartbeat` — connection stays alive for a message.

### `tests/snapshots.rs`
- `test_snapshots_are_bounded_and_oldest_evicted` — snapshot retention cap
  (issue #58).

### `tests/errors.rs`
- `test_unknown_underlying_is_not_found` — 404 `Error::NotFound`.
- `test_unknown_expiration_and_strike_are_not_found` — the expiration + strike 404s.
- `test_unknown_order_status_is_not_found` — unknown order status → 404.
- `test_modify_unknown_order_is_not_found` — modify unknown order → 404.
- `test_cancel_unknown_order_reports_failure` — cancel unknown order → 200
  `success = false` (idempotent delete, not a 404).
- `test_malformed_expiration_is_bad_request` — `banana` / `-5` expirations → 400.
- `test_out_of_range_control_parameters_are_bad_request` — `size_scalar` 1.5,
  `spread_multiplier` 100.0, `directional_skew` 2.0 → 400.
- `test_unauthenticated_order_is_unauthorized` — unauthenticated placement → 401.
- `test_read_token_cannot_place_order` — Read token placement → 403.

## License

MIT
