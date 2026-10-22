-- FDRD-8 / FDRD-9: recurring deposit contributions (grain: savings_account_id).
-- Expected mandatory/recommended deposit amount is configuration from m_deposit_account_recurring_detail;
-- actual deposit amount is the recorded deposit transactions within the window.
-- Expected and actual are never merged into a single "performance" metric.
-- Anti-fanout: actual totals are aggregated in a scalar subquery to preserve account grain.
-- Frequency and calendar linkage fields are surfaced (deposit_period, frequency, is_calendar_inherited).
-- Office scope is the client's office (m_client.office_id) bound inside SQL.
SELECT
    sa.id AS savings_account_id,
    sa.client_id,
    c.office_id,
    sa.currency_code,
    rd.mandatory_recommended_deposit_amount AS expected_deposit_amount,
    COALESCE((
        SELECT SUM(sat.amount)
        FROM m_savings_account_transaction sat
        WHERE sat.savings_account_id = sa.id
          AND sat.transaction_type_enum = 1
          AND NOT sat.is_reversed
          AND sat.transaction_date >= $2::date AND sat.transaction_date <= $3::date
    ), 0.0) AS actual_deposit_amount,
    COALESCE((
        SELECT COUNT(*)
        FROM m_savings_account_transaction sat
        WHERE sat.savings_account_id = sa.id
          AND sat.transaction_type_enum = 1
          AND NOT sat.is_reversed
          AND sat.transaction_date >= $2::date AND sat.transaction_date <= $3::date
    ), 0) AS actual_deposit_count,
    dat.deposit_period,
    dat.deposit_period_frequency_enum::bigint AS deposit_period_frequency_enum,
    rd.is_calendar_inherited,
    rd.is_mandatory
FROM m_savings_account sa
JOIN m_client c ON c.id = sa.client_id
JOIN m_deposit_account_recurring_detail rd ON rd.savings_account_id = sa.id
LEFT JOIN m_deposit_account_term_and_preclosure dat ON dat.savings_account_id = sa.id
WHERE c.office_id = ANY($1::bigint[])
  AND sa.deposit_type_enum = 300
  AND sa.submittedon_date <= $3::date
  AND (sa.closedon_date IS NULL OR sa.closedon_date >= $2::date)
ORDER BY c.office_id, sa.client_id, sa.id
LIMIT $4
