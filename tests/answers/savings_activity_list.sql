-- params: {"from_date": ":month_start", "to_date": ":today", "currency_code": null, "product_ids": null, "limit": 100}
-- Transaksi tabungan bulan berjalan lintas kantor terotorisasi, dibatasi
-- row cap 100 (savings/activity_list.yaml guards.max_limit=hard_cap=100,
-- FIN-133) — nama transaction_type diambil dari VALUES list yang ditulis
-- ULANG dari knowledge/schema/fineract/enums/savings_transaction_type.yaml
-- (kolom `code:`), BUKAN disalin dari CASE produksi
-- (queries/savings/activity_list.sql): CASE produksi salah memetakan 17 ke
-- 'withhold_tax' (harusnya 'overdraft_interest'; 18 tidak dipetakan sama
-- sekali dan jatuh ke 'other'), jadi oracle ini SENGAJA diharapkan gagal
-- sampai FIN-132 memperbaiki queries/savings/activity_list.sql. Nama produk
-- lewat subquery skalar, bukan JOIN ke m_savings_product; klien tetap LEFT
-- JOIN.
WITH transaction_type_labels(transaction_type_enum, transaction_type) AS (
    VALUES
        (0, 'invalid'),
        (1, 'deposit'),
        (2, 'withdrawal'),
        (3, 'interest_posting'),
        (4, 'withdrawal_fee'),
        (5, 'annual_fee'),
        (6, 'waive_charges'),
        (7, 'pay_charge'),
        (8, 'dividend_payout'),
        (10, 'accrual'),
        (12, 'initiate_transfer'),
        (13, 'approve_transfer'),
        (14, 'withdraw_transfer'),
        (15, 'reject_transfer'),
        (16, 'written_off'),
        (17, 'overdraft_interest'),
        (18, 'withhold_tax'),
        (19, 'escheat'),
        (20, 'amount_hold'),
        (21, 'amount_release')
)
SELECT t.id AS transaction_id,
       t.transaction_date,
       t.transaction_type_enum::bigint AS transaction_type_enum,
       coalesce(l.transaction_type, 'other') AS transaction_type,
       t.amount,
       sa.currency_code,
       t.office_id,
       (SELECT o.name FROM m_office o WHERE o.id = t.office_id) AS office_name,
       sa.product_id,
       (SELECT sp.name FROM m_savings_product sp WHERE sp.id = sa.product_id) AS product_name,
       sa.client_id,
       (SELECT c.display_name FROM m_client c WHERE c.id = sa.client_id) AS client_display_name
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id = t.savings_account_id
LEFT JOIN transaction_type_labels l ON l.transaction_type_enum = t.transaction_type_enum
WHERE t.is_reversed = false
  AND t.transaction_date BETWEEN :'month_start'::date AND :'today'::date
  AND t.office_id = ANY(:'office_ids'::bigint[])
ORDER BY t.transaction_date DESC, t.id DESC
LIMIT 100
