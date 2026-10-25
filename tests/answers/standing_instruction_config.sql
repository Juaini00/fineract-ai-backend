-- params: {"limit": 200}
-- FIN-114 D05: standing instruction configuration in authorized offices.
-- Written independently of queries/standing_instructions/standing_instruction_config.sql:
-- resolving office through CTE join, currency via coalesce from source account.
WITH scoped_offices AS (
    SELECT id AS office_id
    FROM m_office
    WHERE id = ANY(:'office_ids'::bigint[])
)
SELECT
    si.id AS standing_instruction_id,
    si.name,
    si.status::smallint AS status,
    si.instruction_type::smallint AS instruction_type,
    si.priority::smallint AS priority,
    si.amount,
    COALESCE(sa.currency_code, la.currency_code) AS currency_code,
    atd.from_office_id,
    atd.to_office_id,
    atd.transfer_type::smallint AS transfer_type,
    si.valid_from,
    si.valid_till,
    si.last_run_date,
    si.account_transfer_details_id
FROM m_account_transfer_standing_instructions si
JOIN m_account_transfer_details atd ON atd.id = si.account_transfer_details_id
LEFT JOIN m_savings_account sa ON sa.id = atd.from_savings_account_id
LEFT JOIN m_loan la ON la.id = atd.from_loan_account_id
WHERE (EXISTS (SELECT 1 FROM scoped_offices so WHERE so.office_id = atd.from_office_id)
    OR EXISTS (SELECT 1 FROM scoped_offices so WHERE so.office_id = atd.to_office_id))
  AND si.status != 3
ORDER BY atd.from_office_id, si.id
LIMIT 200
