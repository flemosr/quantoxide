CREATE TABLE price_history_locf (
    time TIMESTAMPTZ NOT NULL UNIQUE,
    value DOUBLE PRECISION NOT NULL,
    PRIMARY KEY (time)
);

CREATE FUNCTION get_locf_sec(t TIMESTAMPTZ)
RETURNS TIMESTAMPTZ AS $$
BEGIN
    RETURN date_trunc('second', t) + 
           CASE WHEN t > date_trunc('second', t) 
                THEN interval '1 second' 
                ELSE interval '0 seconds' END;
END;
$$ LANGUAGE plpgsql;

INSERT INTO price_history_locf (time, value)
SELECT s.time, t.value
FROM generate_series(
    (SELECT get_locf_sec(min(time)) FROM price_history),
    (SELECT get_locf_sec(max(time)) FROM price_history),
    '1 second'::interval
) AS s(time)
LEFT JOIN LATERAL (
    SELECT value
    FROM price_history
    WHERE time <= s.time
    ORDER BY time DESC
    LIMIT 1
) t ON true
WHERE EXISTS (SELECT 1 FROM price_history LIMIT 1);