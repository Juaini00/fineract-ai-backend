-- params: {"from_date": ":today-12m", "to_date": ":today", "limit": 200}
-- NARROWED to offices 2 and 3 (partial authorization proof): the job body carries office_ids [2,3].
-- FIN-114 D05: standing instruction execution attempts in offices 2 and 3.
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
WHERE ((atd.from_office_id IS NOT NULL AND atd.from_office_id = ANY(ARRAY[2,3]::bigint[]))
    OR (atd.to_office_id IS NOT NULL AND atd.to_office_id = ANY(ARRAY[2,3]::bigint[])))
  AND si.status != 3
  AND h.execution_time >= (:'today'::date - interval '12 months')::date
  AND h.execution_time < :'today'::date + 1
ORDER BY h.execution_time DESC, h.id DESC
LIMIT 200
