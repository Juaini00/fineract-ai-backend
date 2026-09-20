SELECT
    o.id AS office_id,
    o.name AS office_name,
    sa.currency_code,
    COUNT(t.id)::bigint AS transaction_count,
    COALESCE(SUM(CASE WHEN t.transaction_type_enum = 1 THEN t.amount ELSE 0 END), 0)::numeric AS deposit_total,
    COALESCE(SUM(CASE WHEN t.transaction_type_enum = 2 THEN t.amount ELSE 0 END), 0)::numeric AS withdrawal_total
FROM m_office o
JOIN m_savings_account_transaction t ON t.office_id = o.id
JOIN m_savings_account sa ON sa.id = t.savings_account_id
WHERE o.id = ANY($1::bigint[])
  AND t.is_reversed = false
  AND t.transaction_date BETWEEN $2::date AND $3::date
GROUP BY o.id, o.name, sa.currency_code
HAVING COUNT(t.id) > 0
ORDER BY transaction_count DESC, o.id ASC, sa.currency_code ASC
LIMIT $4;
