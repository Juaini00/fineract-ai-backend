-- D15e: recorded INTER-OFFICE cash transfers (m_office_transaction), one row
-- per transfer (grain: office_transaction_id) with transaction_date in the
-- explicit inclusive window. Separate from teller/cashier movements and from
-- client-facing transactions. A transfer is visible only when EVERY recorded
-- office on it (from and/or to) is inside the authorized scope, so an
-- out-of-scope counterpart office is never disclosed; a row with both offices
-- NULL has no office path and is not returned. description is free text and is
-- never selected.
SELECT
    ot.id AS office_transaction_id,
    ot.from_office_id,
    ot.to_office_id,
    ot.transaction_date,
    ot.transaction_amount AS amount,
    ot.currency_code
FROM m_office_transaction ot
WHERE (ot.from_office_id IS NOT NULL OR ot.to_office_id IS NOT NULL)
  AND (ot.from_office_id IS NULL OR ot.from_office_id = ANY($1::bigint[]))
  AND (ot.to_office_id IS NULL OR ot.to_office_id = ANY($1::bigint[]))
  AND ot.transaction_date BETWEEN $2::date AND $3::date
ORDER BY ot.transaction_date DESC, ot.id DESC
LIMIT $4
