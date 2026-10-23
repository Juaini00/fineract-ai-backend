-- LINK-4: actual account transfer transactions and movement evidence (grain: transfer_transaction_id).
-- Details-to-transactions is 1:N; never emit detail x transaction fanout.
-- Office authorization: from_office_id in authorized scope governs visibility (owner side);
-- to_office_id, to_client_id, and destination accounts are evidence only.
-- Amount and transaction_date come from the transaction row, not the detail header.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    att.id AS transfer_transaction_id,
    att.account_transfer_details_id,
    atd.from_office_id,
    atd.to_office_id,
    atd.from_client_id,
    atd.to_client_id,
    atd.from_savings_account_id,
    atd.from_loan_account_id,
    atd.to_savings_account_id,
    atd.to_loan_account_id,
    att.from_savings_transaction_id,
    att.from_loan_transaction_id,
    att.to_savings_transaction_id,
    att.to_loan_transaction_id,
    att.transaction_date,
    att.currency_code,
    att.amount,
    att.is_reversed
FROM m_account_transfer_transaction att
JOIN m_account_transfer_details atd ON atd.id = att.account_transfer_details_id
WHERE atd.from_office_id = ANY($1::bigint[])
ORDER BY atd.from_office_id, att.transaction_date DESC, att.id DESC
LIMIT $2
