-- Loan transactions within a date range, office-scoped directly on this
-- table (unlike m_loan), reversed excluded by default (LOAN-6). Allocation
-- portions reconcile to amount for repayment-type transactions only; a
-- disbursement's amount is not allocated across those columns.
SELECT
    t.id AS transaction_id,
    t.loan_id,
    t.transaction_date,
    t.transaction_type_enum::bigint AS transaction_type_enum,
    CASE t.transaction_type_enum
        WHEN 1 THEN 'disbursement'
        WHEN 2 THEN 'repayment'
        WHEN 3 THEN 'contra'
        WHEN 4 THEN 'waive_interest'
        WHEN 5 THEN 'repayment_at_disbursement'
        WHEN 6 THEN 'writeoff'
        WHEN 7 THEN 'marked_for_rescheduling'
        WHEN 8 THEN 'recovery_repayment'
        WHEN 9 THEN 'waive_charges'
        WHEN 10 THEN 'accrual'
        WHEN 12 THEN 'initiate_transfer'
        WHEN 13 THEN 'approve_transfer'
        WHEN 14 THEN 'withdraw_transfer'
        WHEN 15 THEN 'reject_transfer'
        WHEN 16 THEN 'refund'
        WHEN 17 THEN 'charge_payment'
        WHEN 18 THEN 'refund_for_active_loan'
        WHEN 19 THEN 'income_posting'
        WHEN 20 THEN 'credit_balance_refund'
        WHEN 21 THEN 'merchant_issued_refund'
        WHEN 22 THEN 'payout_refund'
        WHEN 23 THEN 'goodwill_credit'
        WHEN 24 THEN 'charge_refund'
        WHEN 25 THEN 'chargeback'
        WHEN 26 THEN 'charge_adjustment'
        WHEN 27 THEN 'charge_off'
        WHEN 28 THEN 'down_payment'
        WHEN 29 THEN 'reage'
        WHEN 30 THEN 'reamortize'
        WHEN 31 THEN 'interest_payment_waiver'
        WHEN 32 THEN 'accrual_activity'
        WHEN 33 THEN 'interest_refund'
        WHEN 34 THEN 'accrual_adjustment'
        ELSE 'other'
    END AS transaction_type,
    t.amount,
    t.principal_portion_derived,
    t.interest_portion_derived,
    t.fee_charges_portion_derived,
    t.penalty_charges_portion_derived,
    t.overpayment_portion_derived,
    t.office_id
FROM m_loan_transaction t
WHERE t.is_reversed = false
  AND t.transaction_date BETWEEN $2::date AND $3::date
  AND t.office_id = ANY($1::bigint[])
ORDER BY t.transaction_date DESC, t.id DESC
LIMIT $4
