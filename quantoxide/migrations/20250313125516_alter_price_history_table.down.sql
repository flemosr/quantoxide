ALTER TABLE price_history
    ADD COLUMN id SERIAL,
    DROP CONSTRAINT price_history_pkey,
    ADD PRIMARY KEY (id);

CREATE INDEX idx_price_history_time ON price_history (time);