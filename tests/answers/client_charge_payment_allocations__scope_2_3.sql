-- params: {"from_date": "2026-01-01", "to_date": "2026-03-31", "limit": 200}
-- NARROWED to offices 2 and 3 (partial authorization proof): the job body carries office_ids [2,3].
-- FIN-113 D04: client charge payment allocations for offices 2 and 3. Written
-- independently of queries/client/client_charge_payment_allocations.sql: client
-- authorization resolved through CTE for ARRAY[2,3], allocation grain used directly.
WITH client_auth AS (
    SELECT id AS client_id, office_id
    FROM m_client
    WHERE office_id = ANY(ARRAY[2,3]::bigint[])
)
SELECT pb.id AS allocation_id,
       pb.client_charge_id,
       pb.client_transaction_id,
       pb.amount AS allocation_amount,
       ch.currency_code AS charge_currency,
       ct.currency_code AS transaction_currency,
       ct.transaction_date,
       ct.transaction_type_enum::bigint AS transaction_type_enum,
       ct.is_reversed,
       ca.office_id AS client_office_id,
       ct.office_id AS transaction_office_id
FROM m_client_charge_paid_by pb
JOIN m_client_charge cc ON cc.id = pb.client_charge_id
JOIN client_auth ca ON ca.client_id = cc.client_id
JOIN m_charge ch ON ch.id = cc.charge_id
JOIN m_client_transaction ct ON ct.id = pb.client_transaction_id
WHERE daterange(DATE '2026-01-01', DATE '2026-03-31', '[]') @> ct.transaction_date
ORDER BY ct.transaction_date DESC, pb.id DESC
LIMIT 200
