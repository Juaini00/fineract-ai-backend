-- params: {"from_date": ":today-12m", "to_date": ":today", "limit": 100}
-- FIN-108 LOAN-6: loan transactions over the last 12 months, reversed
-- excluded, written independently (CASE branches reordered, no fixed
-- dates — same `:today` token convention as savings_deposit_monthly_breakdown
-- so this never goes stale).
SELECT t.id AS transaction_id, t.loan_id, t.transaction_date,
       t.transaction_type_enum::bigint AS transaction_type_enum,
       CASE WHEN t.transaction_type_enum = 1 THEN 'disbursement'
            WHEN t.transaction_type_enum = 2 THEN 'repayment'
            WHEN t.transaction_type_enum = 3 THEN 'contra'
            WHEN t.transaction_type_enum = 4 THEN 'waive_interest'
            WHEN t.transaction_type_enum = 5 THEN 'repayment_at_disbursement'
            WHEN t.transaction_type_enum = 6 THEN 'writeoff'
            WHEN t.transaction_type_enum = 7 THEN 'marked_for_rescheduling'
            WHEN t.transaction_type_enum = 8 THEN 'recovery_repayment'
            WHEN t.transaction_type_enum = 9 THEN 'waive_charges'
            WHEN t.transaction_type_enum = 10 THEN 'accrual'
            WHEN t.transaction_type_enum = 12 THEN 'initiate_transfer'
            WHEN t.transaction_type_enum = 13 THEN 'approve_transfer'
            WHEN t.transaction_type_enum = 14 THEN 'withdraw_transfer'
            WHEN t.transaction_type_enum = 15 THEN 'reject_transfer'
            WHEN t.transaction_type_enum = 16 THEN 'refund'
            WHEN t.transaction_type_enum = 17 THEN 'charge_payment'
            WHEN t.transaction_type_enum = 18 THEN 'refund_for_active_loan'
            WHEN t.transaction_type_enum = 19 THEN 'income_posting'
            WHEN t.transaction_type_enum = 20 THEN 'credit_balance_refund'
            WHEN t.transaction_type_enum = 21 THEN 'merchant_issued_refund'
            WHEN t.transaction_type_enum = 22 THEN 'payout_refund'
            WHEN t.transaction_type_enum = 23 THEN 'goodwill_credit'
            WHEN t.transaction_type_enum = 24 THEN 'charge_refund'
            WHEN t.transaction_type_enum = 25 THEN 'chargeback'
            WHEN t.transaction_type_enum = 26 THEN 'charge_adjustment'
            WHEN t.transaction_type_enum = 27 THEN 'charge_off'
            WHEN t.transaction_type_enum = 28 THEN 'down_payment'
            WHEN t.transaction_type_enum = 29 THEN 'reage'
            WHEN t.transaction_type_enum = 30 THEN 'reamortize'
            WHEN t.transaction_type_enum = 31 THEN 'interest_payment_waiver'
            WHEN t.transaction_type_enum = 32 THEN 'accrual_activity'
            WHEN t.transaction_type_enum = 33 THEN 'interest_refund'
            WHEN t.transaction_type_enum = 34 THEN 'accrual_adjustment'
            ELSE 'other'
       END AS transaction_type,
       t.amount, t.principal_portion_derived, t.interest_portion_derived,
       t.fee_charges_portion_derived, t.penalty_charges_portion_derived,
       t.overpayment_portion_derived, t.office_id
FROM m_loan_transaction t
WHERE t.is_reversed = false
  AND t.transaction_date >= (:'today'::date - interval '12 months')::date
  AND t.transaction_date <= :'today'::date
  AND t.office_id = ANY(:'office_ids'::bigint[])
ORDER BY t.transaction_date DESC, t.id DESC
LIMIT 100
