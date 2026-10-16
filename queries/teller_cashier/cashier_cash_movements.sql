-- D06: RECORDED cashier cash movements, one row per m_cashier_transactions row
-- (grain: cashier_transaction_id) with txn_date in the explicit inclusive
-- window. Only rows physically stored in m_cashier_transactions: Fineract also
-- derives cash in/out for a cashier from loan, savings and client transactions
-- at read time (TellerManagementReadPlatformServiceImpl); those are NOT here.
-- Office scope = teller office, bound inside the SQL. The currency is the
-- stored currency_code; NULL stays NULL (unknown, never a default currency).
-- txn_note, entity_type and entity_id are not selected: the note is free text
-- and no loan/savings linkage is asserted.
SELECT
    x.id AS cashier_transaction_id,
    x.cashier_id,
    t.office_id,
    CASE x.txn_type
        WHEN 101 THEN 'allocate_cash'
        WHEN 102 THEN 'settle_cash'
        WHEN 103 THEN 'cash_in'
        WHEN 104 THEN 'cash_out'
        ELSE 'other'
    END AS txn_type,
    x.txn_date,
    x.txn_amount AS amount,
    x.currency_code
FROM m_cashier_transactions x
JOIN m_cashiers c ON c.id = x.cashier_id
JOIN m_tellers t ON t.id = c.teller_id
WHERE t.office_id = ANY($1::bigint[])
  AND x.txn_date BETWEEN $2::date AND $3::date
ORDER BY x.txn_date DESC, x.id DESC
LIMIT $4
