-- params: {}
-- Ukuran populasi PENUH (tanpa row cap) untuk organization_office_dormant,
-- dipakai lib/answers.js::expectRowCap (FIN-133). Predikat identik dengan
-- oracle utama (GROUP BY/HAVING) MINUS LIMIT, dibungkus subquery karena
-- HAVING menyaring GRUP, bukan baris.
SELECT count(*) AS n
FROM (
    SELECT o.id
    FROM m_office o
    LEFT JOIN m_savings_account_transaction t
           ON t.office_id = o.id
          AND t.is_reversed = false
          AND t.transaction_date >= :'month_start'::date
          AND t.transaction_date <= :'today'::date
    WHERE o.id = ANY(:'office_ids'::bigint[])
    GROUP BY o.id
    HAVING count(t.id) = 0
) dormant
