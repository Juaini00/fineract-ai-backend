-- params: {"account_number": "Branch 001000000001"}
-- Identitas akun tabungan untuk nomor akun yang persis cocok, dalam scope
-- kantor terotorisasi (ditulis terpisah dari
-- queries/savings/account_identity_lookup.sql: subquery berkorelasi, bukan
-- JOIN eksplisit ke m_client/m_office/m_savings_product).
SELECT sa.id AS savings_account_id,
       '****' || right(sa.account_no, 4) AS masked_account_number,
       sa.client_id,
       (SELECT c.display_name FROM m_client c WHERE c.id = sa.client_id) AS client_display_name,
       (SELECT c.office_id FROM m_client c WHERE c.id = sa.client_id) AS office_id,
       (SELECT o.name FROM m_office o
        WHERE o.id = (SELECT c.office_id FROM m_client c WHERE c.id = sa.client_id)) AS office_name,
       sa.product_id AS savings_product_id,
       (SELECT sp.name FROM m_savings_product sp WHERE sp.id = sa.product_id) AS savings_product_name,
       sa.status_enum::bigint AS savings_status_enum,
       sa.currency_code
FROM m_savings_account sa
WHERE sa.account_no = 'Branch 001000000001'
  AND sa.client_id IN (SELECT c.id FROM m_client c WHERE c.office_id = ANY(:'office_ids'::bigint[]))
ORDER BY sa.id
