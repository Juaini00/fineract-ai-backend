-- params: {"limit": 100}
-- FIN-108 slice 4 LOAN-10: guarantor self-guarantee funding, scope via a
-- UNION of in-scope loan ids. 0 rows on this deployment (no
-- self-guarantee funding configured locally).
WITH scoped AS (
    SELECT l.id FROM m_loan l JOIN m_client c ON c.id = l.client_id WHERE c.office_id = ANY(:'office_ids'::bigint[])
    UNION
    SELECT l.id FROM m_loan l JOIN m_group g ON g.id = l.group_id WHERE l.client_id IS NULL AND g.office_id = ANY(:'office_ids'::bigint[])
)
SELECT
    gf.id AS guarantor_funding_id,
    gf.guarantor_id,
    gu.loan_id,
    (SELECT aa.linked_savings_account_id FROM m_portfolio_account_associations aa WHERE aa.id = gf.account_associations_id AND aa.association_type_enum = 2) AS guarantor_savings_account_id,
    gf.amount,
    gf.amount_released_derived,
    gf.amount_remaining_derived,
    gf.amount_transfered_derived,
    gf.status_enum,
    CASE WHEN gf.status_enum = 100 THEN 'active' WHEN gf.status_enum = 200 THEN 'completed'
         WHEN gf.status_enum = 300 THEN 'withdrawn' WHEN gf.status_enum = 400 THEN 'deleted' ELSE 'other' END AS status_label,
    (SELECT sa.currency_code FROM m_portfolio_account_associations aa JOIN m_savings_account sa ON sa.id = aa.linked_savings_account_id
      WHERE aa.id = gf.account_associations_id AND aa.association_type_enum = 2) AS funding_currency_code,
    (SELECT count(*) FROM m_guarantor_transaction gt WHERE gt.guarantor_fund_detail_id = gf.id AND gt.is_reversed = false) AS guarantor_transaction_count
FROM m_guarantor_funding_details gf
JOIN m_guarantor gu ON gu.id = gf.guarantor_id
JOIN scoped s ON s.id = gu.loan_id
ORDER BY gu.loan_id, gf.id
LIMIT 100
