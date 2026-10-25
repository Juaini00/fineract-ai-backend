-- params: {}
-- Ukuran populasi PENUH (tanpa row cap) untuk command_source_trace (savings_account_id = 37),
-- dipakai lib/answers.js::expectRowCap (FIN-133).
WITH scoped_offices AS (
    SELECT id AS office_id
    FROM m_office
    WHERE id = ANY(:'office_ids'::bigint[])
)
SELECT count(*) AS n
FROM m_portfolio_command_source cs
JOIN scoped_offices so ON so.office_id = cs.office_id
WHERE cs.savings_account_id = 37
  AND cs.made_on_date_utc >= (:'today'::date - interval '12 months')::date
  AND cs.made_on_date_utc < (:'today'::date + 1)
