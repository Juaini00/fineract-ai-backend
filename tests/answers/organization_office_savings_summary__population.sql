-- params: {}
-- Ukuran populasi PENUH (tanpa row cap) untuk
-- organization_office_savings_summary, dipakai
-- lib/answers.js::expectRowCap (FIN-133). Grain = (office, currency_code);
-- predikat identik dengan oracle utama MINUS LIMIT.
SELECT count(*) AS n
FROM (
    SELECT o.id, sa.currency_code
    FROM m_office o
    JOIN m_client c ON c.office_id = o.id
    JOIN m_savings_account sa ON sa.client_id = c.id
    WHERE o.id = ANY(:'office_ids'::bigint[])
    GROUP BY o.id, sa.currency_code
) grains
