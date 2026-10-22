-- FDRD-10: recurring deposit shortfall (grain: savings_account_id).
-- total_overdue_amount and no_of_overdue_installments are stored batch-computed values
-- from m_deposit_account_recurring_detail.
-- RD shortfall is strictly distinct from loan arrears — not derived from loan tables or compared to them.
-- NULL values for un-computed/pending accounts remain NULL and are never coerced to 0.
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
SELECT
    sa.id AS savings_account_id,
    sa.client_id,
    c.office_id,
    sa.currency_code,
    rd.total_overdue_amount,
    rd.no_of_overdue_installments,
    rd.is_mandatory,
    sa.status_enum::bigint AS status_enum
FROM m_savings_account sa
JOIN m_client c ON c.id = sa.client_id
JOIN m_deposit_account_recurring_detail rd ON rd.savings_account_id = sa.id
WHERE c.office_id = ANY($1::bigint[])
  AND sa.deposit_type_enum = 300
  AND sa.submittedon_date <= $3::date
  AND (sa.closedon_date IS NULL OR sa.closedon_date >= $2::date)
ORDER BY c.office_id, sa.client_id, sa.id
LIMIT $4
