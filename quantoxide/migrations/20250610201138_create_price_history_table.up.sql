CREATE TABLE price_history (
    time TIMESTAMPTZ NOT NULL UNIQUE,
    value DOUBLE PRECISION NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
    next TIMESTAMPTZ UNIQUE,
    PRIMARY KEY (time)
);

CREATE INDEX idx_price_history_next_null ON price_history (next)
WHERE
    next IS NULL;
