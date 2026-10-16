-- D06: cashier-to-teller ASSIGNMENTS overlapping an explicit date window.
-- One row per m_cashiers assignment (grain: cashier_id). Office scope is the
-- teller's office (m_tellers.office_id) bound inside the SQL; a cashier row
-- without a teller has no office path and cannot be authorized, so it is not
-- returned. Assignment dates/times are the cashier row's own; the teller's
-- valid_from/valid_to/state are separate facts. NULL start/end date means the
-- bound is not recorded and is returned as NULL (never defaulted). The staff
-- member is identified by surrogate staff_id only: names are PII.
-- Free-text descriptions are never selected.
SELECT
    c.id AS cashier_id,
    c.teller_id,
    t.office_id,
    c.staff_id,
    c.start_date AS assignment_start_date,
    c.end_date AS assignment_end_date,
    c.full_day AS is_full_day,
    NULLIF(c.start_time, '') AS start_time,
    NULLIF(c.end_time, '') AS end_time,
    t.valid_from AS teller_valid_from,
    t.valid_to AS teller_valid_to,
    t.state::bigint AS teller_state_enum,
    CASE t.state
        WHEN 100 THEN 'pending'
        WHEN 300 THEN 'active'
        WHEN 400 THEN 'inactive'
        WHEN 600 THEN 'closed'
        ELSE 'unknown'
    END AS teller_state
FROM m_cashiers c
JOIN m_tellers t ON t.id = c.teller_id
WHERE t.office_id = ANY($1::bigint[])
  AND (c.start_date IS NULL OR c.start_date <= $3::date)
  AND (c.end_date IS NULL OR c.end_date >= $2::date)
ORDER BY t.office_id, c.teller_id, c.start_date, c.id
LIMIT $4
