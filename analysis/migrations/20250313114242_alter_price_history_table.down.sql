ALTER TABLE price_history
    ADD COLUMN id SERIAL,
    DROP CONSTRAINT price_history_pkey,
    ADD PRIMARY KEY (id),
    ADD CONSTRAINT unique_time UNIQUE (time);

CREATE INDEX idx_price_history_time ON price_history (time);