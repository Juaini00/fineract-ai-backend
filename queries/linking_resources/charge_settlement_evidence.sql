-- LINK-6: charge settlement evidence (grain: charge_paid_by_id).
-- Surfaces savings charge settlement evidence linking charges to their paying transactions.
-- Settlement evidence is never inferred from balance drops alone.
-- Loan side has no explicit paid-by table: m_loan_charge.amount_paid_derived is a derived total (LOAN-7).
-- Office scope is the paying transaction's office bound inside SQL.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    sacp.id AS charge_paid_by_id,
    sacp.savings_account_charge_id,
    sacp.savings_account_transaction_id,
    sac.savings_account_id,
    sac.charge_id,
    sacp.amount,
    sat.transaction_date,
    sa.currency_code,
    sat.office_id
FROM m_savings_account_charge_paid_by sacp
JOIN m_savings_account_charge sac ON sac.id = sacp.savings_account_charge_id
JOIN m_savings_account_transaction sat ON sat.id = sacp.savings_account_transaction_id
JOIN m_savings_account sa ON sa.id = sac.savings_account_id
WHERE sat.office_id = ANY($1::bigint[])
ORDER BY sat.office_id, sat.transaction_date DESC, sacp.id DESC
LIMIT $2
