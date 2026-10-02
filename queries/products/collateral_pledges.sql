SELECT cc.id AS client_collateral_id,
  cc.client_id,
  c.office_id,
  cc.collateral_id AS collateral_type_id,
  cc.quantity,
  lc.id AS loan_collateral_id,
  lc.loan_id,
  lc.is_released
FROM m_client_collateral_management cc
JOIN m_client c ON c.id = cc.client_id
LEFT JOIN m_loan_collateral_management lc ON lc.client_collateral_id = cc.id
WHERE c.office_id = ANY($1::bigint[])
ORDER BY cc.id, lc.id;
