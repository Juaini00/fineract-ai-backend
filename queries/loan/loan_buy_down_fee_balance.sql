-- buy-down-fee balance records (dataset-inventory.md LOAN-12, D15: "m_loan_buy_down_fee_balance"
-- named explicitly by dataset-scope-decisions.md:188). Grain: one row per
-- m_loan_buy_down_fee_balance record = one buy-down-fee source transaction
-- (unique (loan_id, loan_transaction_id); source transaction type 40 per Apache
-- Fineract LoanTransactionType). Kept as its own capability -- never joined to
-- the other D15 balance table, so no multi-child fanout.
-- Semantics from Apache Fineract source (LoanAdjustmentServiceImpl,
-- Loan*AmortizationProcessingServiceImpl):
--   is_deleted = true  -> the source transaction was reversed; the record is
--                         excluded here (a reversed event never counts), and a
--                         reversed source transaction is excluded as well.
--   is_closed  = true  -> amortization finished (fully recognized, or the loan
--                         closed/charged off); kept and labelled, not hidden.
--   amount_adjustment  -> sum of buy-down-fee adjustment transactions against it.
--   unrecognized_amount-> portion not yet amortized into income.
--   charged_off_amount -> unrecognized portion moved out at charge-off.
-- Amounts are in the loan's currency (currency_code per row). Office-scoped via
-- the owning loan's client office, else group office.
SELECT
    b.id AS balance_id,
    b.loan_id,
    b.loan_transaction_id AS source_transaction_id,
    t.transaction_date AS source_transaction_date,
    b.date AS balance_date,
    b.amount,
    b.amount_adjustment,
    b.unrecognized_amount,
    b.charged_off_amount,
    b.is_closed,
    l.currency_code
FROM m_loan_buy_down_fee_balance b
JOIN m_loan l ON l.id = b.loan_id
JOIN m_loan_transaction t ON t.id = b.loan_transaction_id
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
  AND b.is_deleted = false
  AND t.is_reversed = false
ORDER BY b.loan_id, b.date, b.id
LIMIT $2
