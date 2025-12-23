-- Initial database schema for Option Chain OrderBook Backend

-- Underlying prices table
CREATE TABLE IF NOT EXISTS underlying_prices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    symbol VARCHAR(20) NOT NULL,
    price_cents BIGINT NOT NULL,
    bid_cents BIGINT,
    ask_cents BIGINT,
    volume BIGINT,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    source VARCHAR(50),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for fast symbol lookups
CREATE INDEX IF NOT EXISTS idx_underlying_prices_symbol ON underlying_prices(symbol);
CREATE INDEX IF NOT EXISTS idx_underlying_prices_timestamp ON underlying_prices(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_underlying_prices_symbol_timestamp ON underlying_prices(symbol, timestamp DESC);

-- Market maker configuration per symbol
CREATE TABLE IF NOT EXISTS market_maker_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    symbol VARCHAR(20) NOT NULL UNIQUE,
    quoting_enabled BOOLEAN NOT NULL DEFAULT true,
    spread_multiplier DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    size_scalar DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    directional_skew DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    max_position BIGINT NOT NULL DEFAULT 1000,
    max_delta DOUBLE PRECISION NOT NULL DEFAULT 100.0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- System-wide control settings (singleton table)
CREATE TABLE IF NOT EXISTS system_control (
    id INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    master_enabled BOOLEAN NOT NULL DEFAULT true,
    global_spread_multiplier DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    global_size_scalar DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    global_directional_skew DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Insert default system control row
INSERT INTO system_control (id, master_enabled, global_spread_multiplier, global_size_scalar, global_directional_skew)
VALUES (1, true, 1.0, 1.0, 0.0)
ON CONFLICT (id) DO NOTHING;

-- Execution audit trail
CREATE TABLE IF NOT EXISTS executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id VARCHAR(50) NOT NULL,
    symbol VARCHAR(20) NOT NULL,
    instrument VARCHAR(100) NOT NULL,
    side VARCHAR(10) NOT NULL,
    quantity BIGINT NOT NULL,
    price_cents BIGINT NOT NULL,
    theo_value_cents BIGINT,
    edge_cents BIGINT,
    latency_us BIGINT,
    executed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for execution queries
CREATE INDEX IF NOT EXISTS idx_executions_symbol ON executions(symbol);
CREATE INDEX IF NOT EXISTS idx_executions_executed_at ON executions(executed_at DESC);
CREATE INDEX IF NOT EXISTS idx_executions_order_id ON executions(order_id);

-- Latest price view for quick lookups
CREATE OR REPLACE VIEW latest_underlying_prices AS
SELECT DISTINCT ON (symbol)
    id, symbol, price_cents, bid_cents, ask_cents, volume, timestamp, source, created_at
FROM underlying_prices
ORDER BY symbol, timestamp DESC;
