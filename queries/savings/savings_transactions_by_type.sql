-- SAV-5: savings transaction totals and counts broken down by transaction type (grain: currency_code, transaction_type_enum).
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
-- m_savings_account_transaction.office_id is evidence only and not used for filtering.
-- Transaction types 1-21 follow SavingsAccountTransactionType.java:35-54:
-- DEPOSIT=1, WITHDRAWAL=2, INTEREST_POSTING=3, WITHDRAWAL_FEE=4, ANNUAL_FEE=5,
-- WAIVE_CHARGES=6, PAY_CHARGE=7, DIVIDEND_PAYOUT=8, ACCRUAL=10, INITIATE_TRANSFER=12,
-- APPROVE_TRANSFER=13, WITHDRAW_TRANSFER=14, REJECT_TRANSFER=15, WRITTEN_OFF=16,
-- OVERDRAFT_INTEREST=17, WITHHOLD_TAX=18, ESCHEAT=19, AMOUNT_HOLD=20, AMOUNT_RELEASE=21.
-- Types 22-26 remain raw ::bigint without invented labels.
-- Reversed transactions are excluded by default (is_reversed = false).
-- Grouped per currency code to prevent cross-currency summation (FIN-107 decision 1).
SELECT
    sa.currency_code,
    sat.transaction_type_enum::bigint AS transaction_type_enum,
    COUNT(sat.id)::bigint AS transaction_count,
    COALESCE(SUM(sat.amount), 0)::numeric AS total_amount
FROM m_savings_account_transaction sat
JOIN m_savings_account sa ON sa.id = sat.savings_account_id
JOIN m_client c ON c.id = sa.client_id
WHERE c.office_id = ANY($1::bigint[])
  AND sat.is_reversed = false
  AND sat.transaction_date >= $2::date AND sat.transaction_date <= $3::date
GROUP BY sa.currency_code, sat.transaction_type_enum
ORDER BY sa.currency_code, sat.transaction_type_enum
LIMIT $4
