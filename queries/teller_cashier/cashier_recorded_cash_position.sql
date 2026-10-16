-- D06: RECORDED cash position per cashier and currency, cumulative from the
-- beginning through the declared cutoff $3 = to_date (grain: cashier_id +
-- currency_code), with the position just before the window start $2 = from_date
-- so the movement inside the window is visible. The planner contract requires
-- both date slots for an explicit period; both are meaningful here.
-- Fineract's own formula (TellerManagementReadPlatformServiceImpl summary):
--   position = allocate(101) + cash in - cash out - settle(102)
-- where cash in/out are the stored journal rows (103/104) PLUS the cash Fineract
-- derives from savings, loan and client transactions attributed to the cashier:
--   * attribution is INFERRED: creating user's staff (m_appuser.staff_id) equals
--     the cashier's staff_id AND the transaction date lies inside the cashier's
--     own assignment dates (savings: end_date; loan/client: end_date + 1 day, as
--     in the Fineract reader); there is no cashier FK;
--   * ANTI-FANOUT: every derived event is classified BEFORE summing by the number
--     of cashier assignments that match it (counted over ALL assignments, in or
--     out of scope, because ambiguity is a property of the event). Exactly one
--     match = admitted. More than one = AMBIGUOUS: the event is added to no
--     cashier's amounts (no arbitrary choice, no double counting), it is counted
--     in ambiguous_event_count of every in-scope candidate group, and the
--     position of such a group is withheld (NULL, position_status
--     'withheld_ambiguous_attribution'), never a confident number;
--   * only an EXPLICIT cash payment type (is_cash_payment = true) counts; a NULL
--     payment detail does not prove cash and is NOT counted (the reader would);
--   * reversed rows and account transfers are excluded (client: reversed only,
--     as in the reader); savings deposit / withdrawal, loan repayment-type
--     (2,5,8,17) = in and disbursement (1) = out, client PAY_CHARGE (1) = in.
--     Waivers, write-offs and interest are not cash and are not counted.
-- Scope: the event's own office must be authorized (inside the event query) and
-- only candidate cashiers whose teller office is authorized are returned, so no
-- out-of-scope cashier, amount or count is exposed. A group exists when any row
-- exists for it, so a cashier/currency that nets to zero is returned as 0, not
-- dropped; a NULL journal currency is its own group. Recorded system data, not
-- counted physical cash.
WITH journal AS (
    SELECT x.cashier_id, t.office_id, x.currency_code, x.txn_date AS d,
           CASE x.txn_type WHEN 101 THEN 'allocated' WHEN 102 THEN 'settled'
                           WHEN 103 THEN 'journal_in' WHEN 104 THEN 'journal_out'
                           ELSE 'unrecognized' END AS kind,
           x.txn_amount AS amount
    FROM m_cashier_transactions x
    JOIN m_cashiers c ON c.id = x.cashier_id
    JOIN m_tellers t ON t.id = c.teller_id
    WHERE t.office_id = ANY($1::bigint[])
      AND x.txn_date <= $3::date
), candidates AS (
    SELECT 'savings' AS src, st.id AS event_id, c.id AS cashier_id, t.office_id,
           sa.currency_code, st.transaction_date AS d,
           CASE st.transaction_type_enum WHEN 1 THEN 'savings_in' ELSE 'savings_out' END AS kind,
           st.amount
    FROM m_savings_account_transaction st
    JOIN m_savings_account sa ON sa.id = st.savings_account_id
    JOIN m_appuser u ON u.id = st.created_by
    JOIN m_cashiers c ON c.staff_id = u.staff_id
                     AND st.transaction_date BETWEEN c.start_date AND c.end_date
    JOIN m_tellers t ON t.id = c.teller_id
    JOIN m_payment_detail pd ON pd.id = st.payment_detail_id
    JOIN m_payment_type pt ON pt.id = pd.payment_type_id
    WHERE pt.is_cash_payment = true
      AND st.is_reversed = false
      AND st.transaction_type_enum IN (1, 2)
      AND st.transaction_date <= $3::date
      AND st.office_id = ANY($1::bigint[])
      AND NOT EXISTS (
          SELECT 1 FROM m_account_transfer_transaction tr
          WHERE tr.from_savings_transaction_id = st.id OR tr.to_savings_transaction_id = st.id)
    UNION ALL
    SELECT 'loan', lt.id, c.id, t.office_id, l.currency_code, lt.transaction_date,
           CASE WHEN lt.transaction_type_enum = 1 THEN 'loan_out' ELSE 'loan_in' END,
           lt.amount
    FROM m_loan_transaction lt
    JOIN m_loan l ON l.id = lt.loan_id
    JOIN m_appuser u ON u.id = lt.created_by
    JOIN m_cashiers c ON c.staff_id = u.staff_id
                     AND lt.transaction_date BETWEEN c.start_date AND c.end_date + 1
    JOIN m_tellers t ON t.id = c.teller_id
    JOIN m_payment_detail pd ON pd.id = lt.payment_detail_id
    JOIN m_payment_type pt ON pt.id = pd.payment_type_id
    WHERE pt.is_cash_payment = true
      AND lt.is_reversed = false
      AND lt.transaction_type_enum IN (1, 2, 5, 8, 17)
      AND lt.transaction_date <= $3::date
      AND lt.office_id = ANY($1::bigint[])
      AND NOT EXISTS (
          SELECT 1 FROM m_account_transfer_transaction tr
          WHERE tr.from_loan_transaction_id = lt.id OR tr.to_loan_transaction_id = lt.id)
    UNION ALL
    SELECT 'client', ct.id, c.id, t.office_id, ct.currency_code, ct.transaction_date,
           'client_in', ct.amount
    FROM m_client_transaction ct
    JOIN m_appuser u ON u.id = ct.created_by
    JOIN m_cashiers c ON c.staff_id = u.staff_id
                     AND ct.transaction_date BETWEEN c.start_date AND c.end_date + 1
    JOIN m_tellers t ON t.id = c.teller_id
    JOIN m_payment_detail pd ON pd.id = ct.payment_detail_id
    JOIN m_payment_type pt ON pt.id = pd.payment_type_id
    WHERE pt.is_cash_payment = true
      AND ct.is_reversed = false
      AND ct.transaction_type_enum = 1
      AND ct.transaction_date <= $3::date
      AND ct.office_id = ANY($1::bigint[])
), classified AS (
    SELECT cd.*, COUNT(*) OVER (PARTITION BY cd.src, cd.event_id) AS matching_assignments
    FROM candidates cd
), flow AS (
    SELECT cashier_id, office_id, currency_code, d, kind, amount, true AS admitted FROM journal
    UNION ALL
    SELECT cashier_id, office_id, currency_code, d, kind, amount, (matching_assignments = 1)
    FROM classified
    WHERE office_id = ANY($1::bigint[])
), signed AS (
    SELECT f.*,
           CASE WHEN NOT f.admitted THEN 0
                WHEN f.kind IN ('settled', 'journal_out', 'savings_out', 'loan_out') THEN -f.amount
                WHEN f.kind = 'unrecognized' THEN 0
                ELSE f.amount END AS signed_amount
    FROM flow f
), pos AS (
    SELECT cashier_id, office_id, currency_code,
        COALESCE(SUM(amount) FILTER (WHERE admitted AND kind = 'allocated'), 0) AS allocated_amount,
        COALESCE(SUM(amount) FILTER (WHERE admitted AND kind = 'settled'), 0) AS settled_amount,
        COALESCE(SUM(amount) FILTER (WHERE admitted AND kind = 'journal_in'), 0) AS journal_cash_in_amount,
        COALESCE(SUM(amount) FILTER (WHERE admitted AND kind = 'journal_out'), 0) AS journal_cash_out_amount,
        COALESCE(SUM(amount) FILTER (WHERE admitted AND kind = 'savings_in'), 0) AS savings_cash_in_amount,
        COALESCE(SUM(amount) FILTER (WHERE admitted AND kind = 'savings_out'), 0) AS savings_cash_out_amount,
        COALESCE(SUM(amount) FILTER (WHERE admitted AND kind = 'loan_in'), 0) AS loan_cash_in_amount,
        COALESCE(SUM(amount) FILTER (WHERE admitted AND kind = 'loan_out'), 0) AS loan_cash_out_amount,
        COALESCE(SUM(amount) FILTER (WHERE admitted AND kind = 'client_in'), 0) AS client_cash_in_amount,
        COALESCE(SUM(signed_amount), 0) AS summed_position_amount,
        COALESCE(SUM(signed_amount) FILTER (WHERE d < $2::date), 0) AS summed_before_window_amount,
        COUNT(*) FILTER (WHERE NOT admitted) AS ambiguous_event_count,
        COUNT(*) FILTER (WHERE NOT admitted AND d < $2::date) AS ambiguous_before_window_count,
        COUNT(*) FILTER (WHERE admitted AND kind = 'unrecognized') AS unrecognized_type_count
    FROM signed
    GROUP BY cashier_id, office_id, currency_code
)
SELECT
    p.cashier_id,
    p.office_id,
    p.currency_code,
    $2::date AS window_start_date,
    $3::date AS as_of_date,
    CASE WHEN p.ambiguous_before_window_count = 0 THEN p.summed_before_window_amount END AS position_before_window_amount,
    p.allocated_amount,
    p.settled_amount,
    p.journal_cash_in_amount,
    p.journal_cash_out_amount,
    p.savings_cash_in_amount,
    p.savings_cash_out_amount,
    p.loan_cash_in_amount,
    p.loan_cash_out_amount,
    p.client_cash_in_amount,
    CASE WHEN p.ambiguous_event_count = 0 THEN p.summed_position_amount END AS recorded_cash_position_amount,
    p.ambiguous_event_count,
    CASE WHEN p.ambiguous_event_count = 0 THEN 'complete' ELSE 'withheld_ambiguous_attribution' END AS position_status,
    p.unrecognized_type_count,
    'recorded position: allocate + cash in - cash out - settle, cumulative through as_of_date; position_before_window is the same position before the window start; cash in/out include stored journal rows and, for transactions made by the cashier''s staff inside the assignment dates with an explicit cash payment type, savings, loan and client transactions (attribution inferred; payment details that are not explicitly cash are not counted); an event matching more than one assignment is counted in ambiguous_event_count and added to no amount, and the position of an affected group is withheld; allocated, settled and cash in/out columns are cumulative through as_of_date and include admitted events only; not counted physical cash' AS position_basis
FROM pos p
ORDER BY p.office_id, p.cashier_id, p.currency_code NULLS LAST
LIMIT $4
