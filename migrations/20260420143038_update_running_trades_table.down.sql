TRUNCATE TABLE running_trades;

ALTER TABLE running_trades
    DROP CONSTRAINT running_trades_pkey;

ALTER TABLE running_trades
    ADD PRIMARY KEY (trade_id);

ALTER TABLE running_trades
    DROP COLUMN account_id;
