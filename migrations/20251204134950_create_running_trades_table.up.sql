CREATE TABLE running_trades (
    trade_id UUID NOT NULL PRIMARY KEY,
    trailing_stoploss DOUBLE PRECISION,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
    CONSTRAINT trailing_stoploss_bounded CHECK (
        trailing_stoploss IS NULL OR (trailing_stoploss >= 0.1 AND trailing_stoploss <= 99.9)
    )
);
