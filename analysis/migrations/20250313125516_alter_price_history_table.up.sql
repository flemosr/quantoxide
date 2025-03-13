DROP INDEX idx_price_history_time;

ALTER TABLE price_history
    DROP CONSTRAINT price_history_pkey,
    ADD PRIMARY KEY (time),
    DROP COLUMN id;