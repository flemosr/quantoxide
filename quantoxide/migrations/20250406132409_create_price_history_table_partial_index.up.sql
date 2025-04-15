CREATE INDEX idx_price_history_next_null ON price_history (next)
WHERE
    next IS NULL;
