-- params: {"office_name": "Head Office"}
-- Cek keberadaan kantor bernama tertentu; active_staff_count dihitung lewat
-- LEFT JOIN LATERAL, berbeda dari queries/organization/office_name_lookup.sql
-- yang memakai subquery berkorelasi di SELECT list.
SELECT
    count(*)::bigint AS office_count,
    count(*) FILTER (WHERE o.parent_id IS NULL)::bigint AS root_office_count,
    min(o.opening_date) AS oldest_opening_date,
    coalesce(sum(staff.active_count), 0)::bigint AS active_staff_count
FROM m_office o
LEFT JOIN LATERAL (
    SELECT count(*) AS active_count
    FROM m_staff s
    WHERE s.office_id = o.id AND s.is_active = true
) staff ON true
WHERE o.id = ANY(:'office_ids'::bigint[])
  AND lower(o.name) = lower('Head Office')
