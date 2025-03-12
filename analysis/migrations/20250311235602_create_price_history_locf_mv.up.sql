CREATE MATERIALIZED VIEW price_history_locf_mv AS
SELECT s.second, t.value
FROM generate_series(
    (SELECT date_trunc('second', min(time)) FROM price_history),
    (SELECT max(time) FROM price_history),
    '1 second'::interval
) AS s(second)
LEFT JOIN LATERAL (
    SELECT value
    FROM price_history
    WHERE time <= s.second
    ORDER BY time DESC
    LIMIT 1
) t ON true;