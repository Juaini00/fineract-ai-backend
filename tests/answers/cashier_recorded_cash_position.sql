-- params: {"from_date": "2026-04-01", "to_date": "2026-04-30", "limit": 200}
-- FIN-115 D06: recorded cash position per cashier and currency through 2026-04-30
-- (cumulative), with the position before the window start 2026-04-01.
-- Independent of queries/teller_cashier/cashier_recorded_cash_position.sql, and
-- independently ANTI-FANOUT: here the number of matching assignments of each derived
-- event is a correlated scalar subquery over m_cashiers (all assignments, in or out
-- of scope); only events with exactly one match are attributed, an event with several
-- is ambiguous and withholds the position of every in-scope candidate group. The
-- (cashier, currency) pairs are the union of all branches and every measure is its
-- own scalar subquery. Expected (coordinator source report): cashier 3 / USD = 0,
-- cashier 2 / USD 20800, cashier 4 / USD 35970 and AED 25000; locally no event is
-- ambiguous (verified: staff ids are unique per assignment), so every status is
-- complete. The synthetic two-assignment case is covered by
-- tests/position_attribution_smoke.sh.
WITH jr AS (
    SELECT c.id AS cid, t.office_id AS oid, x.currency_code AS cur, x.txn_type AS k, x.txn_amount AS amt, x.txn_date AS dt
    FROM m_cashiers c, m_tellers t, m_cashier_transactions x
    WHERE t.id = c.teller_id AND x.cashier_id = c.id
      AND t.office_id = ANY(:'office_ids'::bigint[])
      AND x.txn_date <= DATE '2026-04-30'
), sv AS (
    SELECT c.id AS cid, t.office_id AS oid, sa.currency_code AS cur, st.transaction_type_enum AS k, st.amount AS amt, st.transaction_date AS dt,
           (SELECT COUNT(*) FROM m_cashiers c2 JOIN m_appuser u2 ON u2.staff_id = c2.staff_id
             WHERE u2.id = st.created_by AND c2.start_date <= st.transaction_date AND st.transaction_date <= c2.end_date) AS matches
    FROM m_cashiers c
    JOIN m_tellers t ON t.id = c.teller_id
    JOIN m_appuser u ON u.staff_id = c.staff_id
    JOIN m_savings_account_transaction st ON st.created_by = u.id
    JOIN m_savings_account sa ON sa.id = st.savings_account_id
    WHERE t.office_id = ANY(:'office_ids'::bigint[]) AND st.office_id = ANY(:'office_ids'::bigint[])
      AND st.transaction_date <= DATE '2026-04-30'
      AND c.start_date <= st.transaction_date AND st.transaction_date <= c.end_date
      AND st.is_reversed IS NOT TRUE AND st.transaction_type_enum IN (1, 2)
      AND EXISTS (SELECT 1 FROM m_payment_detail pd JOIN m_payment_type pt ON pt.id = pd.payment_type_id
                  WHERE pd.id = st.payment_detail_id AND pt.is_cash_payment IS TRUE)
      AND NOT EXISTS (SELECT 1 FROM m_account_transfer_transaction x WHERE st.id IN (x.from_savings_transaction_id, x.to_savings_transaction_id))
), ln AS (
    SELECT c.id AS cid, t.office_id AS oid, l.currency_code AS cur, lt.transaction_type_enum AS k, lt.amount AS amt, lt.transaction_date AS dt,
           (SELECT COUNT(*) FROM m_cashiers c2 JOIN m_appuser u2 ON u2.staff_id = c2.staff_id
             WHERE u2.id = lt.created_by AND c2.start_date <= lt.transaction_date AND lt.transaction_date <= c2.end_date + 1) AS matches
    FROM m_cashiers c
    JOIN m_tellers t ON t.id = c.teller_id
    JOIN m_appuser u ON u.staff_id = c.staff_id
    JOIN m_loan_transaction lt ON lt.created_by = u.id
    JOIN m_loan l ON l.id = lt.loan_id
    WHERE t.office_id = ANY(:'office_ids'::bigint[]) AND lt.office_id = ANY(:'office_ids'::bigint[])
      AND lt.transaction_date <= DATE '2026-04-30'
      AND c.start_date <= lt.transaction_date AND lt.transaction_date <= c.end_date + 1
      AND lt.is_reversed IS NOT TRUE AND lt.transaction_type_enum IN (1, 2, 5, 8, 17)
      AND EXISTS (SELECT 1 FROM m_payment_detail pd JOIN m_payment_type pt ON pt.id = pd.payment_type_id
                  WHERE pd.id = lt.payment_detail_id AND pt.is_cash_payment IS TRUE)
      AND NOT EXISTS (SELECT 1 FROM m_account_transfer_transaction x WHERE lt.id IN (x.from_loan_transaction_id, x.to_loan_transaction_id))
), cl AS (
    SELECT c.id AS cid, t.office_id AS oid, ct.currency_code AS cur, ct.amount AS amt, ct.transaction_date AS dt,
           (SELECT COUNT(*) FROM m_cashiers c2 JOIN m_appuser u2 ON u2.staff_id = c2.staff_id
             WHERE u2.id = ct.created_by AND c2.start_date <= ct.transaction_date AND ct.transaction_date <= c2.end_date + 1) AS matches
    FROM m_cashiers c
    JOIN m_tellers t ON t.id = c.teller_id
    JOIN m_appuser u ON u.staff_id = c.staff_id
    JOIN m_client_transaction ct ON ct.created_by = u.id
    WHERE t.office_id = ANY(:'office_ids'::bigint[]) AND ct.office_id = ANY(:'office_ids'::bigint[])
      AND ct.transaction_date <= DATE '2026-04-30'
      AND c.start_date <= ct.transaction_date AND ct.transaction_date <= c.end_date + 1
      AND ct.is_reversed IS NOT TRUE AND ct.transaction_type_enum = 1
      AND EXISTS (SELECT 1 FROM m_payment_detail pd JOIN m_payment_type pt ON pt.id = pd.payment_type_id
                  WHERE pd.id = ct.payment_detail_id AND pt.is_cash_payment IS TRUE)
), pairs AS (
    SELECT DISTINCT cid, oid, cur FROM (
        SELECT cid, oid, cur FROM jr UNION ALL SELECT cid, oid, cur FROM sv
        UNION ALL SELECT cid, oid, cur FROM ln UNION ALL SELECT cid, oid, cur FROM cl) u
), m AS (
    SELECT p.cid, p.oid, p.cur,
        COALESCE((SELECT SUM(amt) FROM jr WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND k = 101), 0) AS a,
        COALESCE((SELECT SUM(amt) FROM jr WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND k = 102), 0) AS s,
        COALESCE((SELECT SUM(amt) FROM jr WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND k = 103), 0) AS ji,
        COALESCE((SELECT SUM(amt) FROM jr WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND k = 104), 0) AS jo,
        COALESCE((SELECT SUM(amt) FROM sv WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches = 1 AND k = 1), 0) AS si,
        COALESCE((SELECT SUM(amt) FROM sv WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches = 1 AND k = 2), 0) AS so,
        COALESCE((SELECT SUM(amt) FROM ln WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches = 1 AND k <> 1), 0) AS li,
        COALESCE((SELECT SUM(amt) FROM ln WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches = 1 AND k = 1), 0) AS lo,
        COALESCE((SELECT SUM(amt) FROM cl WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches = 1), 0) AS ci,
        (SELECT COUNT(*) FROM jr WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND k NOT IN (101, 102, 103, 104)) AS un,
        (SELECT COUNT(*) FROM sv WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches > 1)
      + (SELECT COUNT(*) FROM ln WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches > 1)
      + (SELECT COUNT(*) FROM cl WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches > 1) AS amb,
        (SELECT COUNT(*) FROM sv WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches > 1 AND dt < DATE '2026-04-01')
      + (SELECT COUNT(*) FROM ln WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches > 1 AND dt < DATE '2026-04-01')
      + (SELECT COUNT(*) FROM cl WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches > 1 AND dt < DATE '2026-04-01') AS amb_before,
        -- position before the window start: the same signed sum over admitted rows dated before 2026-04-01
        COALESCE((SELECT SUM(amt) FROM jr WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND dt < DATE '2026-04-01' AND k IN (101, 103)), 0)
      - COALESCE((SELECT SUM(amt) FROM jr WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND dt < DATE '2026-04-01' AND k IN (102, 104)), 0)
      + COALESCE((SELECT SUM(amt) FROM sv WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches = 1 AND dt < DATE '2026-04-01' AND k = 1), 0)
      - COALESCE((SELECT SUM(amt) FROM sv WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches = 1 AND dt < DATE '2026-04-01' AND k = 2), 0)
      + COALESCE((SELECT SUM(amt) FROM ln WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches = 1 AND dt < DATE '2026-04-01' AND k <> 1), 0)
      - COALESCE((SELECT SUM(amt) FROM ln WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches = 1 AND dt < DATE '2026-04-01' AND k = 1), 0)
      + COALESCE((SELECT SUM(amt) FROM cl WHERE cid = p.cid AND cur IS NOT DISTINCT FROM p.cur AND matches = 1 AND dt < DATE '2026-04-01'), 0) AS before_window
    FROM pairs p
)
SELECT cid AS cashier_id, oid AS office_id, cur AS currency_code,
       DATE '2026-04-01' AS window_start_date,
       DATE '2026-04-30' AS as_of_date,
       CASE WHEN amb_before = 0 THEN before_window END AS position_before_window_amount,
       a AS allocated_amount, s AS settled_amount,
       ji AS journal_cash_in_amount, jo AS journal_cash_out_amount,
       si AS savings_cash_in_amount, so AS savings_cash_out_amount,
       li AS loan_cash_in_amount, lo AS loan_cash_out_amount,
       ci AS client_cash_in_amount,
       CASE WHEN amb = 0 THEN (a + ji + si + li + ci) - (jo + so + lo + s) END AS recorded_cash_position_amount,
       amb AS ambiguous_event_count,
       CASE WHEN amb = 0 THEN 'complete' ELSE 'withheld_ambiguous_attribution' END AS position_status,
       un AS unrecognized_type_count,
       'recorded position: allocate + cash in - cash out - settle, cumulative through as_of_date; position_before_window is the same position before the window start; cash in/out include stored journal rows and, for transactions made by the cashier''s staff inside the assignment dates with an explicit cash payment type, savings, loan and client transactions (attribution inferred; payment details that are not explicitly cash are not counted); an event matching more than one assignment is counted in ambiguous_event_count and added to no amount, and the position of an affected group is withheld; allocated, settled and cash in/out columns are cumulative through as_of_date and include admitted events only; not counted physical cash' AS position_basis
FROM m
ORDER BY oid, cid, (cur IS NULL), cur
