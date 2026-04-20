TRUNCATE TABLE running_trades;

ALTER TABLE running_trades
    ADD COLUMN account_id UUID NOT NULL;

ALTER TABLE running_trades
    DROP CONSTRAINT running_trades_pkey;

ALTER TABLE running_trades
    ADD PRIMARY KEY (account_id, trade_id);
