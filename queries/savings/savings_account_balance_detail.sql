-- SAV-4: savings account four-measure balance detail, holds, and overdraft (grain: account_id).
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
-- Four distinct balance measures are separately surfaced and never merged:
-- account_balance_derived, available_balance_derived, total_savings_amount_on_hold, on_hold_funds_derived.
-- allow_overdraft and overdraft_limit are also surfaced per account.
-- Currency per code without cross-currency consolidation (FIN-107 decision 1).
-- Account numbers and external IDs are never selected (pii safe).
SELECT
    sa.id AS account_id,
    sa.client_id,
    c.office_id,
    sa.product_id,
    sp.name AS product_name,
    sa.currency_code,
    sa.account_balance_derived,
    sa.available_balance_derived,
    sa.total_savings_amount_on_hold,
    sa.on_hold_funds_derived,
    sa.allow_overdraft,
    sa.overdraft_limit,
    sa.submittedon_date
FROM m_savings_account sa
JOIN m_client c ON c.id = sa.client_id
JOIN m_savings_product sp ON sp.id = sa.product_id
WHERE c.office_id = ANY($1::bigint[])
  AND sa.submittedon_date >= $2::date AND sa.submittedon_date <= $3::date
ORDER BY c.office_id, sa.client_id, sa.id
LIMIT $4
