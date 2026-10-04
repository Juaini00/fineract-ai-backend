-- Recovery payments on written-off loans (dataset-inventory.md LOAN-11):
-- m_loan_transaction filtered to transaction_type_enum = 8 (recovery_repayment
-- per Apache Fineract LoanTransactionType; local r_enum_value stops at 19 but
-- this value is within that range, verified FIN-108). Reversed transactions
-- are excluded by default so a reversed recovery never counts toward cash
-- totals (same rule as loan.loan_transactions, LOAN-6). Amounts are in the
-- owning loan's currency (currency_code). office_id is direct
-- on this table, not re-derived through the client/group path. 0 rows on
-- this deployment; positive behaviour proven by a read-only fixture, see
-- knowledge/VERIFICATION.md.
SELECT
    t.id AS transaction_id,
    t.loan_id,
    t.transaction_date,
    t.amount,
    t.principal_portion_derived,
    t.interest_portion_derived,
    t.fee_charges_portion_derived,
    t.penalty_charges_portion_derived,
    t.office_id,
    l.currency_code
FROM m_loan_transaction t
JOIN m_loan l ON l.id = t.loan_id
WHERE t.transaction_type_enum = 8
  AND t.is_reversed = false
  AND t.office_id = ANY($1::bigint[])
ORDER BY t.transaction_date DESC, t.id DESC
LIMIT $2
