-- params: {"currency_code": null, "limit": null}
-- Saldo tabungan per kantor dan mata uang (L1.1), diurutkan dari saldo
-- terbesar. INNER JOIN dipakai supaya kantor tanpa akun tersingkir secara
-- alami, berbeda dari queries/organization/office_savings_summary.sql yang
-- LEFT JOIN lalu HAVING.
SELECT
    o.id AS office_id,
    o.name AS office_name,
    sa.currency_code,
    count(sa.id) FILTER (WHERE sa.status_enum = 300)::bigint AS active_account_count,
    count(sa.id)::bigint AS total_account_count,
    coalesce(sum(sa.account_balance_derived) FILTER (WHERE sa.status_enum = 300), 0)::numeric AS total_balance
FROM m_office o
JOIN m_client c ON c.office_id = o.id
JOIN m_savings_account sa ON sa.client_id = c.id
WHERE o.id = ANY(:'office_ids'::bigint[])
GROUP BY o.id, o.name, sa.currency_code
ORDER BY total_balance DESC, o.id ASC
