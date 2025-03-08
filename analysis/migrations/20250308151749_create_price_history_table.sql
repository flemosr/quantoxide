-- Create price history table
CREATE TABLE IF NOT EXISTS price_history (
    id SERIAL PRIMARY KEY,
    time TIMESTAMPTZ NOT NULL UNIQUE,
    value DOUBLE PRECISION NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Add index on time for faster time-based queries
CREATE INDEX idx_price_history_time ON price_history (time);