-- params: {}
-- Full population size for account_transfer_transactions without row cap.
SELECT count(*) AS n
FROM m_account_transfer_transaction att
JOIN m_account_transfer_details atd ON atd.id = att.account_transfer_details_id
WHERE atd.from_office_id = ANY(:'office_ids'::bigint[])
