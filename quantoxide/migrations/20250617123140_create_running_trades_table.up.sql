CREATE TABLE running_trades (
    trade_id UUID PRIMARY KEY,
    trailing_stoploss DOUBLE PRECISION CHECK (
        trailing_stoploss >= 0.1
        AND trailing_stoploss <= 99.9
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW ()
);
