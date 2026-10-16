-- params: {"from_date": "2026-02-01", "to_date": "2026-02-28", "limit": 200}
-- FIN-115 D06: cashier assignments overlapping 2026-02-01..2026-02-28 for every
-- authorized office. Written independently of queries/teller_cashier/
-- cashier_assignments.sql: the overlap is a daterange intersection (NULL bound =
-- open), tellers drive the join, state labels come from a VALUES list. Cashier 4
-- (2025-11-30..2025-12-31) ends before the window and must be absent.
WITH state_label(state, label) AS (
    VALUES (100, 'pending'), (300, 'active'), (400, 'inactive'), (600, 'closed')
)
SELECT c.id AS cashier_id,
       t.id AS teller_id,
       t.office_id,
       c.staff_id,
       c.start_date AS assignment_start_date,
       c.end_date AS assignment_end_date,
       c.full_day AS is_full_day,
       CASE WHEN c.start_time = '' THEN NULL ELSE c.start_time END AS start_time,
       CASE WHEN c.end_time = '' THEN NULL ELSE c.end_time END AS end_time,
       t.valid_from AS teller_valid_from,
       t.valid_to AS teller_valid_to,
       t.state::bigint AS teller_state_enum,
       COALESCE(sl.label, 'unknown') AS teller_state
FROM m_tellers t
JOIN m_cashiers c ON c.teller_id = t.id
LEFT JOIN state_label sl ON sl.state = t.state
WHERE t.office_id = ANY(:'office_ids'::bigint[])
  AND daterange(c.start_date, c.end_date, '[]') && daterange(DATE '2026-02-01', DATE '2026-02-28', '[]')
ORDER BY t.office_id, t.id, c.start_date, c.id
