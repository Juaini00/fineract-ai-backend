-- params: {}
-- Ringkasan kantor + staf aktif, lewat LEFT JOIN LATERAL alih-alih subquery
-- berkorelasi di SELECT list (queries/organization/office_summary.sql).
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
