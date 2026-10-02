-- params: {}
-- FIN-109 PROD-5: independent Fineract source comparison.
SELECT pledge.id client_collateral_id, owner.id client_id, owner.office_id, pledge.collateral_id collateral_type_id, pledge.quantity, loan_link.id loan_collateral_id, loan_link.loan_id, loan_link.is_released
FROM m_client owner
JOIN m_client_collateral_management pledge ON pledge.client_id=owner.id
LEFT
JOIN m_loan_collateral_management loan_link ON loan_link.client_collateral_id=pledge.id
WHERE owner.office_id=ANY(:'office_ids'::bigint[])
ORDER BY pledge.id,loan_link.id
