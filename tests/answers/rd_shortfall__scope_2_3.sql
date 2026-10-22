-- params: {"from_date": "2026-01-01", "to_date": "2026-03-31", "limit": 200}
-- NARROWED to offices 2 and 3: the job body carries office_ids [2,3].
-- FDRD-10: recurring deposit shortfall for offices 2 and 3.
WITH client_owner AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(ARRAY[2,3]::bigint[])
)
SELECT sa.id AS savings_account_id,
       sa.client_id,
       co.office_id,
       sa.currency_code,
       rd.total_overdue_amount,
       rd.no_of_overdue_installments,
       rd.is_mandatory,
       sa.status_enum::bigint AS status_enum
FROM m_savings_account sa
JOIN client_owner co ON co.client_id = sa.client_id
JOIN m_deposit_account_recurring_detail rd ON rd.savings_account_id = sa.id
WHERE sa.deposit_type_enum = 300
  AND sa.submittedon_date <= DATE '2026-03-31'
  AND (sa.closedon_date IS NULL OR sa.closedon_date >= DATE '2026-01-01')
ORDER BY co.office_id, sa.client_id, sa.id
LIMIT 200
