CREATE TABLE price_history (
    id SERIAL PRIMARY KEY,
    time TIMESTAMPTZ NOT NULL UNIQUE,
    value DOUBLE PRECISION NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    next TIMESTAMPTZ UNIQUE
);

CREATE INDEX idx_price_history_time ON price_history (time);