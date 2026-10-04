-- Guarantor funding (self-guarantee hold on a savings account),
-- dataset-inventory.md LOAN-10. m_guarantor_funding_details.account_associations_id
-- is a FK to m_portfolio_account_associations; per Apache Fineract
-- GuarantorWritePlatformServiceJpaRepositoryIImpl the funding row is created
-- with AccountAssociations.associateSavingsAccount(loan, savingsAccount,
-- GUARANTOR_ACCOUNT_ASSOCIATION = 2), i.e. the held guarantor savings account
-- is linked_savings_account_id. funding_currency_code is THAT savings
-- account's currency; it is NULL (never guessed from the loan) when the
-- association is missing or not a guarantor association. amount_released/
-- remaining/transfered are Fineract's own derived columns, read directly.
-- status per GuarantorFundStatusType (100 active, 200 completed, 300 withdrawn,
-- 400 deleted). guarantor_transaction_count is a scalar subquery over
-- non-reversed m_guarantor_transaction rows, never a join, so the funding row
-- is not fanned out. 0 rows on this deployment; positive behaviour proven by a
-- read-only fixture, see knowledge/VERIFICATION.md.
SELECT
    gf.id AS guarantor_funding_id,
    gf.guarantor_id,
    gu.loan_id,
    aa.linked_savings_account_id AS guarantor_savings_account_id,
    gf.amount,
    gf.amount_released_derived,
    gf.amount_remaining_derived,
    gf.amount_transfered_derived,
    gf.status_enum,
    CASE gf.status_enum
        WHEN 100 THEN 'active'
        WHEN 200 THEN 'completed'
        WHEN 300 THEN 'withdrawn'
        WHEN 400 THEN 'deleted'
        ELSE 'other'
    END AS status_label,
    sa.currency_code AS funding_currency_code,
    (SELECT COUNT(*) FROM m_guarantor_transaction gt WHERE gt.guarantor_fund_detail_id = gf.id AND gt.is_reversed = false) AS guarantor_transaction_count
FROM m_guarantor_funding_details gf
JOIN m_guarantor gu ON gu.id = gf.guarantor_id
JOIN m_loan l ON l.id = gu.loan_id
LEFT JOIN m_portfolio_account_associations aa
    ON aa.id = gf.account_associations_id AND aa.association_type_enum = 2
LEFT JOIN m_savings_account sa ON sa.id = aa.linked_savings_account_id
LEFT JOIN m_client c ON c.id = l.client_id
LEFT JOIN m_group g ON g.id = l.group_id
WHERE COALESCE(c.office_id, g.office_id) = ANY($1::bigint[])
ORDER BY gu.loan_id, gf.id
LIMIT $2
