-- params: {"from_date": ":today", "to_date": ":today"}
-- Independent answer for the capability's default single-day window. The
-- office set is the local authorized admin projection, not caller input.
WITH scheduled AS (
    SELECT ci.entity_id, ci.entity_type_enum, c.id AS calendar_id,
           c.start_date, c.end_date, c.repeating, c.recurrence,
           c.meeting_time::text AS meeting_time
    FROM m_calendar c
    JOIN m_calendar_instance ci ON ci.calendar_id = c.id
    WHERE c.calendar_type_enum = 1
      AND c.start_date <= :'today'::date
      AND (c.end_date IS NULL OR c.end_date >= :'today'::date)
      AND ((c.repeating AND c.recurrence = 'FREQ=DAILY')
           OR (NOT c.repeating AND c.start_date = :'today'::date))
)
SELECT g.id AS group_id, gl.level_name AS group_level_name, g.office_id,
       g.staff_id, s.calendar_id, :'today'::date AS scheduled_date,
       s.start_date AS meeting_start_date, s.end_date AS meeting_end_date,
       s.repeating AS meeting_repeating, s.recurrence AS meeting_recurrence,
       s.meeting_time, gc.client_id AS member_client_id
FROM scheduled s
JOIN m_group g ON g.id = s.entity_id
JOIN m_group_level gl ON gl.id = g.level_id
LEFT JOIN m_group_client gc ON gc.group_id = g.id AND gl.can_have_clients
WHERE g.office_id = ANY(:'office_ids'::bigint[])
  AND ((gl.can_have_clients AND s.entity_type_enum = 2)
       OR (NOT gl.can_have_clients AND s.entity_type_enum = 4))
ORDER BY scheduled_date, g.id, s.calendar_id, gc.client_id
