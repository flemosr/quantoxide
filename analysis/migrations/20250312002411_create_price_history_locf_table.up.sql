CREATE TABLE price_history_locf (
    time TIMESTAMPTZ NOT NULL UNIQUE,
    value DOUBLE PRECISION NOT NULL,
    PRIMARY KEY (time)
);

INSERT INTO price_history_locf (time, value)
SELECT s.time, t.value
FROM generate_series(
    (SELECT date_trunc('second', min(time)) + '1 second'::interval FROM price_history),
    (SELECT max(time) FROM price_history),
    '1 second'::interval
) AS s(time)
LEFT JOIN LATERAL (
    SELECT value
    FROM price_history
    WHERE time <= s.time
    ORDER BY time DESC
    LIMIT 1
) t ON true
WHERE EXISTS (
    SELECT 1
    FROM price_history
    OFFSET 1 LIMIT 1
);