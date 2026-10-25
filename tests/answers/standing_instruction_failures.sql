-- params: {"from_date": ":today-12m", "to_date": ":today", "limit": 200}
-- FIN-114 D05: failed standing instruction execution attempts in authorized offices.
-- Written independently of queries/standing_instructions/standing_instruction_failures.sql:
-- resolving office through CTE join, history status filtered to 'failed'.
WITH scoped_offices AS (
    SELECT id AS office_id
    FROM m_office
    WHERE id = ANY(:'office_ids'::bigint[])
)
SELECT
    h.id AS history_id,
    h.standing_instruction_id,
    si.name AS name,
    h.status,
    to_char(h.execution_time, 'YYYY-MM-DD"T"HH24:MI:SS.US') AS execution_time,
    h.amount,
    h.error_log
FROM m_account_transfer_standing_instructions_history h
JOIN m_account_transfer_standing_instructions si ON si.id = h.standing_instruction_id
JOIN m_account_transfer_details atd ON atd.id = si.account_transfer_details_id
WHERE (EXISTS (SELECT 1 FROM scoped_offices so WHERE so.office_id = atd.from_office_id)
    OR EXISTS (SELECT 1 FROM scoped_offices so WHERE so.office_id = atd.to_office_id))
  AND si.status != 3
  AND h.status = 'failed'
  AND h.execution_time >= (:'today'::date - interval '12 months')::date
  AND h.execution_time < :'today'::date + 1
ORDER BY h.execution_time DESC, h.id DESC
LIMIT 200
