CREATE TABLE indicators (
    time TIMESTAMPTZ NOT NULL UNIQUE,
    value DOUBLE PRECISION NOT NULL,
    ma_5 DOUBLE PRECISION,
    ma_60 DOUBLE PRECISION,
    ma_300 DOUBLE PRECISION,
    PRIMARY KEY (time)
);

INSERT INTO indicators (time, value, ma_5, ma_60, ma_300)
WITH price_data AS (
    SELECT time, value, ROW_NUMBER() OVER (ORDER BY time) AS rn
    FROM price_history_locf
    ORDER BY time ASC
)
SELECT
    time,
    value,
    CASE 
        WHEN rn >= 5
        THEN AVG(value) OVER (ROWS BETWEEN 4 PRECEDING AND CURRENT ROW)
        ELSE NULL
    END AS ma_5,
    CASE
        WHEN rn >= 60
        THEN AVG(value) OVER (ROWS BETWEEN 59 PRECEDING AND CURRENT ROW)
        ELSE NULL
    END AS ma_60,
    CASE
        WHEN rn >= 300
        THEN AVG(value) OVER (ROWS BETWEEN 299 PRECEDING AND CURRENT ROW)
        ELSE NULL
    END AS ma_300
FROM price_data;