-- params: {"limit": 200}
-- FIN-130 LINK-4: actual account transfer transactions and movement evidence in authorized offices.
-- Written independently of queries/linking_resources/account_transfer_transactions.sql:
-- from-office scope resolved via CTE filter on m_account_transfer_details.
WITH details_scope AS (
    SELECT id AS details_id, from_office_id, to_office_id, from_client_id, to_client_id,
           from_savings_account_id, from_loan_account_id, to_savings_account_id, to_loan_account_id
    FROM m_account_transfer_details
    WHERE from_office_id = ANY(:'office_ids'::bigint[])
)
SELECT
    att.id AS transfer_transaction_id,
    att.account_transfer_details_id,
    ds.from_office_id,
    ds.to_office_id,
    ds.from_client_id,
    ds.to_client_id,
    ds.from_savings_account_id,
    ds.from_loan_account_id,
    ds.to_savings_account_id,
    ds.to_loan_account_id,
    att.from_savings_transaction_id,
    att.from_loan_transaction_id,
    att.to_savings_transaction_id,
    att.to_loan_transaction_id,
    att.transaction_date,
    att.currency_code,
    att.amount,
    att.is_reversed
FROM m_account_transfer_transaction att
JOIN details_scope ds ON ds.details_id = att.account_transfer_details_id
ORDER BY ds.from_office_id, att.transaction_date DESC, att.id DESC
LIMIT 200
