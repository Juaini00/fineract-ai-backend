-- params: {"limit": 100}
-- FIN-108 slice 4 LOAN-12 (D15): m_loan_buy_down_fee_balance records, scope via a UNION of
-- in-scope loan ids; reversed-source records excluded with NOT EXISTS instead
-- of the production join filter.
WITH scoped AS (
    SELECT l.id FROM m_loan l JOIN m_client c ON c.id = l.client_id WHERE c.office_id = ANY(:'office_ids'::bigint[])
    UNION
    SELECT l.id FROM m_loan l JOIN m_group g ON g.id = l.group_id WHERE l.client_id IS NULL AND g.office_id = ANY(:'office_ids'::bigint[])
)
SELECT
    b.id AS balance_id,
    b.loan_id,
    b.loan_transaction_id AS source_transaction_id,
    (SELECT t.transaction_date FROM m_loan_transaction t WHERE t.id = b.loan_transaction_id) AS source_transaction_date,
    b.date AS balance_date,
    b.amount,
    b.amount_adjustment,
    b.unrecognized_amount,
    b.charged_off_amount,
    b.is_closed,
    (SELECT l.currency_code FROM m_loan l WHERE l.id = b.loan_id) AS currency_code
FROM m_loan_buy_down_fee_balance b
JOIN scoped s ON s.id = b.loan_id
WHERE NOT b.is_deleted
  AND NOT EXISTS (SELECT 1 FROM m_loan_transaction t WHERE t.id = b.loan_transaction_id AND t.is_reversed)
ORDER BY b.loan_id, b.date, b.id
LIMIT 100
