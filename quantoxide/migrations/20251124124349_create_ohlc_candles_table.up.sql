CREATE TABLE ohlc_candles (
    time TIMESTAMPTZ NOT NULL PRIMARY KEY ,
    open DOUBLE PRECISION NOT NULL,
    high DOUBLE PRECISION NOT NULL,
    low DOUBLE PRECISION NOT NULL,
    close DOUBLE PRECISION NOT NULL,
    volume BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
    gap BOOLEAN NOT NULL DEFAULT FALSE,
    stable BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE INDEX idx_ohlc_candles_gaps ON ohlc_candles (gap)
WHERE gap IS TRUE;

CREATE OR REPLACE FUNCTION update_ohlc_candles_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_ohlc_candles_updated_at
BEFORE UPDATE ON ohlc_candles
FOR EACH ROW
EXECUTE FUNCTION update_ohlc_candles_updated_at();
