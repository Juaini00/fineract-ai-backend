-- params: {}
-- Full population size for standing_instruction_failures without row cap in the default window.
SELECT count(*) AS n
FROM m_account_transfer_standing_instructions_history h
JOIN m_account_transfer_standing_instructions si ON si.id = h.standing_instruction_id
JOIN m_account_transfer_details atd ON atd.id = si.account_transfer_details_id
WHERE ((atd.from_office_id IS NOT NULL AND atd.from_office_id = ANY(:'office_ids'::bigint[]))
    OR (atd.to_office_id IS NOT NULL AND atd.to_office_id = ANY(:'office_ids'::bigint[])))
  AND si.status != 3
  AND h.status = 'failed'
  AND h.execution_time >= (:'today'::date - interval '12 months')::date
  AND h.execution_time < :'today'::date + 1
