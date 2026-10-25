-- params: {}
-- Full population size for provisioning_entries in authorized offices without row cap.
WITH scoped_offices AS (
    SELECT id AS office_id
    FROM m_office
    WHERE id = ANY(:'office_ids'::bigint[])
)
SELECT count(*) AS n
FROM m_loanproduct_provisioning_entry e
JOIN scoped_offices so ON so.office_id = e.office_id
JOIN m_provisioning_history h ON h.id = e.history_id
JOIN m_provisioning_criteria c ON c.id = e.criteria_id
WHERE h.created_date >= (:'today'::date - interval '12 months')::date
  AND h.created_date <= :'today'::date
