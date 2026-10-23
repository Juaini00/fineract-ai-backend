-- params: {}
-- Ukuran populasi PENUH (tanpa row cap) untuk client_account_roster,
-- dipakai lib/answers.js::expectRowCap (FIN-133). Predikat identik
-- dengan oracle utama MINUS LIMIT.
SELECT count(*) AS n
FROM (
  SELECT 'loan' AS account_type, l.id AS account_id
  FROM m_loan l
  JOIN m_client c ON c.id = l.client_id
  WHERE l.client_id IS NOT NULL
    AND c.office_id = ANY(:'office_ids'::bigint[])

  UNION ALL

  SELECT 'savings' AS account_type, s.id AS account_id
  FROM m_savings_account s
  JOIN m_client c ON c.id = s.client_id
  WHERE s.client_id IS NOT NULL
    AND c.office_id = ANY(:'office_ids'::bigint[])

  UNION ALL

  SELECT 'share' AS account_type, sh.id AS account_id
  FROM m_share_account sh
  JOIN m_client c ON c.id = sh.client_id
  WHERE sh.client_id IS NOT NULL
    AND c.office_id = ANY(:'office_ids'::bigint[])
) accounts
