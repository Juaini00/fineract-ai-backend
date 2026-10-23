-- LINK-5: loan payment allocation components per transaction (grain: loan_transaction_id).
-- Surfaces principal, interest, fee charges, and penalty charges portion breakdown.
-- Allocation-consistency contract: components sum to amount.
-- Loan side only; savings transactions are single-purpose with no multi-portion equivalent.
-- transaction_type_enum cited to LoanTransactionType.java:26-75:
-- DISBURSEMENT=1, REPAYMENT=2, CONTRA=3, WAIVE_INTEREST=4, REPAYMENT_AT_DISBURSEMENT=5,
-- WRITEOFF=6, MARKED_FOR_RESCHEDULING=7, RECOVERY_REPAYMENT=8, WAIVE_CHARGES=9, ACCRUAL=10,
-- INITIATE_TRANSFER=12, APPROVE_TRANSFER=13, WITHDRAW_TRANSFER=14, REJECT_TRANSFER=15, REFUND=16,
-- CHARGE_PAYMENT=17, REFUND_FOR_ACTIVE_LOAN=18, INCOME_POSTING=19, CREDIT_BALANCE_REFUND=20,
-- MERCHANT_ISSUED_REFUND=21, PAYOUT_REFUND=22, GOODWILL_CREDIT=23, CHARGE_REFUND=24,
-- CHARGEBACK=25, CHARGE_ADJUSTMENT=26, CHARGE_OFF=27, DOWN_PAYMENT=28, REAGE=29, REAMORTIZE=30,
-- INTEREST_PAYMENT_WAIVER=31, ACCRUAL_ACTIVITY=32, INTEREST_REFUND=33, ACCRUAL_ADJUSTMENT=34,
-- CAPITALIZED_INCOME=35, CAPITALIZED_INCOME_AMORTIZATION=36, CAPITALIZED_INCOME_ADJUSTMENT=37.
-- Returned as raw ::bigint.
-- Office scope is the transaction's office bound inside SQL.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    lt.id AS loan_transaction_id,
    lt.loan_id,
    lt.office_id,
    lt.transaction_date,
    lt.transaction_type_enum::bigint AS transaction_type_enum,
    lt.amount,
    lt.principal_portion_derived,
    lt.interest_portion_derived,
    lt.fee_charges_portion_derived,
    lt.penalty_charges_portion_derived,
    l.currency_code,
    lt.is_reversed
FROM m_loan_transaction lt
JOIN m_loan l ON l.id = lt.loan_id
WHERE lt.office_id = ANY($1::bigint[])
ORDER BY lt.office_id, lt.transaction_date DESC, lt.id DESC
LIMIT $2
