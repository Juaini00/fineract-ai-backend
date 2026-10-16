-- params: {"from_date": "2026-01-01", "to_date": "2026-03-31", "limit": 200}
-- FIN-115 D06: recorded m_cashier_transactions rows dated 2026-01-01..2026-03-31,
-- newest first. Independent of queries/teller_cashier/cashier_cash_movements.sql:
-- cashier -> teller -> office is resolved through a subquery, the type label
-- through a VALUES list, the window through a daterange containment test.
WITH label(code, name) AS (
    VALUES (101, 'allocate_cash'), (102, 'settle_cash'), (103, 'cash_in'), (104, 'cash_out')
), owner AS (
    SELECT c.id AS cashier_id, c.teller_id, c.staff_id, t.office_id
    FROM m_cashiers c, m_tellers t
    WHERE t.id = c.teller_id AND t.office_id = ANY(:'office_ids'::bigint[])
)
SELECT x.id AS cashier_transaction_id,
       x.cashier_id,
       o.office_id,
       COALESCE(l.name, 'other') AS txn_type,
       x.txn_date,
       x.txn_amount AS amount,
       x.currency_code
FROM m_cashier_transactions x
JOIN owner o ON o.cashier_id = x.cashier_id
LEFT JOIN label l ON l.code = x.txn_type
WHERE daterange(DATE '2026-01-01', DATE '2026-03-31', '[]') @> x.txn_date
ORDER BY x.txn_date DESC, x.id DESC
