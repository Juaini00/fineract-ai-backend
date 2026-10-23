-- SAV-2: savings account lifecycle, dormant and block status (grain: account_id).
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
-- sub_status_enum mapped via SavingsAccountSubStatusEnum.java:24-30:
-- NONE=0, INACTIVE=100, DORMANT=200, ESCHEAT=300, BLOCK=400, BLOCK_CREDIT=500, BLOCK_DEBIT=600.
-- status_enum mapped via SavingsAccountStatusType.java:29-40:
-- SUBMITTED_AND_PENDING_APPROVAL=100, APPROVED=200, ACTIVE=300, CLOSED=600, etc.
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    sa.id AS account_id,
    sa.client_id,
    c.office_id,
    sa.product_id,
    sp.name AS product_name,
    sa.status_enum::bigint AS status_enum,
    sa.sub_status_enum::bigint AS sub_status_enum,
    sa.reason_for_block,
    sa.submittedon_date,
    sa.approvedon_date,
    sa.rejectedon_date,
    sa.withdrawnon_date,
    sa.activatedon_date,
    sa.closedon_date,
    sa.currency_code
FROM m_savings_account sa
JOIN m_client c ON c.id = sa.client_id
JOIN m_savings_product sp ON sp.id = sa.product_id
WHERE c.office_id = ANY($1::bigint[])
  AND sa.submittedon_date >= $2::date AND sa.submittedon_date <= $3::date
ORDER BY c.office_id, sa.client_id, sa.id
LIMIT $4
