-- params: {"from_date": "2026-01-01", "to_date": "2026-03-31", "limit": 200}
-- FDRD-1 / FDRD-4: fixed and recurring deposit accounts for authorized offices.
-- Written independently of queries/fixed_recurring_deposit/fd_rd_accounts.sql:
-- client office resolved through client_owner CTE, deposit types 200 (FD) and 300 (RD).
WITH client_owner AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(:'office_ids'::bigint[])
)
SELECT sa.id AS savings_account_id,
       sa.client_id,
       co.office_id,
       sa.deposit_type_enum::bigint AS deposit_type_enum,
       sa.status_enum::bigint AS status_enum,
       sa.product_id,
       sa.currency_code,
       sa.nominal_annual_interest_rate,
       sa.total_interest_posted_derived,
       sa.submittedon_date,
       sa.activatedon_date,
       sa.closedon_date
FROM m_savings_account sa
JOIN client_owner co ON co.client_id = sa.client_id
WHERE sa.deposit_type_enum IN (200, 300)
  AND (sa.submittedon_date >= DATE '2026-01-01' AND sa.submittedon_date <= DATE '2026-03-31')
ORDER BY co.office_id, sa.client_id, sa.id
LIMIT 200
