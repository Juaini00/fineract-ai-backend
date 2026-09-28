SELECT
    g.id AS group_id,
    gl.level_name AS group_level_name,
    g.office_id,
    g.staff_id,
    c.id AS calendar_id,
    occurrence.scheduled_date::date AS scheduled_date,
    c.start_date AS meeting_start_date,
    c.end_date AS meeting_end_date,
    c.repeating AS meeting_repeating,
    c.recurrence AS meeting_recurrence,
    c.meeting_time::text AS meeting_time,
    gc.client_id AS member_client_id
FROM m_group g
JOIN m_group_level gl ON gl.id = g.level_id
JOIN m_calendar_instance ci
    ON ci.entity_id = g.id
   AND ((ci.entity_type_enum = 2 AND gl.can_have_clients)
     OR (ci.entity_type_enum = 4 AND NOT gl.can_have_clients))
JOIN m_calendar c ON c.id = ci.calendar_id AND c.calendar_type_enum = 1
CROSS JOIN LATERAL generate_series(
    GREATEST(c.start_date, $1::date),
    LEAST(COALESCE(c.end_date, $2::date), $2::date),
    INTERVAL '1 day'
) AS occurrence(scheduled_date)
LEFT JOIN m_group_client gc ON gc.group_id = g.id AND gl.can_have_clients
WHERE g.office_id = ANY($3::bigint[])
  AND ($1::date <= $2::date)
  AND (c.repeating AND c.recurrence = 'FREQ=DAILY'
       OR NOT c.repeating AND occurrence.scheduled_date::date = c.start_date)
ORDER BY occurrence.scheduled_date, g.id, c.id, gc.client_id;
