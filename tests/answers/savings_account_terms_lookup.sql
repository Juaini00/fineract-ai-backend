-- params: {"account_number": "Branch 001000000001"}
-- Suku bunga dan syarat overdraft akun & produk untuk nomor akun yang persis
-- cocok (ditulis terpisah dari queries/savings/account_terms_lookup.sql:
-- scope kantor lewat EXISTS, bukan JOIN ke m_client).
SELECT sa.id AS savings_account_id,
       '****' || right(sa.account_no, 4) AS masked_account_number,
       sa.nominal_annual_interest_rate AS account_nominal_annual_interest_rate,
       sp.nominal_annual_interest_rate AS product_nominal_annual_interest_rate,
       sa.allow_overdraft AS account_allow_overdraft,
       sp.allow_overdraft AS product_allow_overdraft,
       sa.overdraft_limit AS account_overdraft_limit,
       sp.overdraft_limit AS product_overdraft_limit
FROM m_savings_account sa
JOIN m_savings_product sp ON sp.id = sa.product_id
WHERE sa.account_no = 'Branch 001000000001'
  AND EXISTS (
      SELECT 1 FROM m_client c
      WHERE c.id = sa.client_id AND c.office_id = ANY(:'office_ids'::bigint[])
  )
ORDER BY sa.id
