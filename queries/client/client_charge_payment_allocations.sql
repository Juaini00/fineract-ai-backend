-- D04: client charge PAYMENT ALLOCATIONS, one row per m_client_charge_paid_by
-- allocation row (grain: allocation_id). Allocation grain is the safe measure;
-- summing m_client_transaction.amount across joined allocations is forbidden to
-- avoid fan-out and double counting. Office authorization path is the client's
-- office (m_client.office_id) bound inside the SQL; transaction_office_id is
-- returned as evidence in its own column and never confers authorization. Charge
-- currency (m_charge.currency_code) and payment transaction currency
-- (m_client_transaction.currency_code) are shown separately with no cross-currency
-- conversion or consolidation. Transaction reversal (is_reversed) is surfaced
-- explicitly, never silently netted or filtered. Client names and free-text
-- descriptions are never selected.
SELECT
    pb.id AS allocation_id,
    pb.client_charge_id,
    pb.client_transaction_id,
    pb.amount AS allocation_amount,
    ch.currency_code AS charge_currency,
    ct.currency_code AS transaction_currency,
    ct.transaction_date,
    ct.transaction_type_enum::bigint AS transaction_type_enum,
    ct.is_reversed,
    c.office_id AS client_office_id,
    ct.office_id AS transaction_office_id
FROM m_client_charge_paid_by pb
JOIN m_client_charge cc ON cc.id = pb.client_charge_id
JOIN m_client c ON c.id = cc.client_id
JOIN m_charge ch ON ch.id = cc.charge_id
JOIN m_client_transaction ct ON ct.id = pb.client_transaction_id
WHERE c.office_id = ANY($1::bigint[])
  AND ct.transaction_date BETWEEN $2::date AND $3::date
ORDER BY ct.transaction_date DESC, pb.id DESC
LIMIT $4
