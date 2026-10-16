-- params: {"from_date": "2026-01-01", "to_date": "2026-06-30", "limit": 200}
-- FIN-115 D15e: recorded inter-office cash transfers dated 2026-01-01..2026-06-30
-- whose every recorded office is authorized. Independent of
-- queries/teller_cashier/inter_office_cash_transfers.sql: the visible offices are
-- an array, and a row qualifies when its non-NULL offices are a subset of it and
-- it has at least one. The reference deployment holds no such row (0 in
-- m_office_transaction), so the expected answer is the empty set.
SELECT ot.id AS office_transaction_id,
       ot.from_office_id,
       ot.to_office_id,
       ot.transaction_date,
       ot.transaction_amount AS amount,
       ot.currency_code
FROM m_office_transaction ot
WHERE array_remove(ARRAY[ot.from_office_id, ot.to_office_id], NULL) <@ (:'office_ids'::bigint[])
  AND cardinality(array_remove(ARRAY[ot.from_office_id, ot.to_office_id], NULL)) > 0
  AND ot.transaction_date >= DATE '2026-01-01' AND ot.transaction_date <= DATE '2026-06-30'
ORDER BY ot.transaction_date DESC, ot.id DESC
