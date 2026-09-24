-- params: {"currency_code": null, "limit": 10}
-- Ranking nasabah dengan saldo tabungan tertinggi per mata uang, LATERAL per
-- klien alih-alih JOIN+GROUP BY datar seperti file queries aslinya.
SELECT
    cl.id AS client_id,
    o.id AS office_id,
    o.name AS office_name,
    bal.currency_code,
    bal.n AS account_count,
    bal.total AS total_balance
FROM m_client cl
JOIN m_office o ON o.id = cl.office_id
JOIN LATERAL (
    SELECT s.currency_code, count(*) AS n, coalesce(sum(s.account_balance_derived), 0) AS total
    FROM m_savings_account s
    WHERE s.client_id = cl.id AND s.status_enum = 300
    GROUP BY s.currency_code
    HAVING coalesce(sum(s.account_balance_derived), 0) > 0
) bal ON true
WHERE cl.office_id = ANY(:'office_ids'::bigint[])
ORDER BY bal.total DESC, cl.id ASC
LIMIT 10
