# Verifikasi katalog (L1) — hasil pengukuran, bukan klaim

Dokumen ini mencatat hasil penerapan **empat aturan** di
[CARRY-OVER.md](CARRY-OVER.md) §"Aturan sebelum sebuah entri dianggap sah",
ditambah butir yang [queries/CARRY-OVER.md](../queries/CARRY-OVER.md) nyatakan
belum pernah diperiksa siapa pun (dua kelas timeout, grain/anti-fanout,
penamaan kolom, semantik mata uang).

Aturan mainnya: **tidak ada entri yang boleh ditandai `lulus` tanpa dua angka
berdampingan** — angka yang dihasilkan kapabilitas dan angka yang dihasilkan SQL
langsung yang ditulis terpisah.

- Tanggal: 2026-09-15
- Basis data: `fineract_default` lokal, **read-only** (hanya `SELECT`).
- Kebenaran dasar: **8 kantor, 43 klien, 15.607 transaksi**
  (`m_office` / `m_client` / `m_savings_account_transaction`).
- Cara menjalankan: tiap `sql_file` disiapkan dengan `PREPARE` lalu dijalankan
  dengan `EXECUTE` memakai parameter di bawah, langsung lewat `psql`.
  `cargo run -p app -- catalog` dipakai untuk gerbang muat/prepare, bukan untuk
  kebenaran angka — ia sendiri menyatakan itu pada bagian "Cakupan pemeriksaan".

## Parameter yang dipakai (himpunan P)

| Parameter | Nilai |
| --- | --- |
| `office_ids` | `ARRAY[1,2,3,4,5,6,8,9]` (seluruh kantor) |
| `from_date` / `to_date` | `2025-01-01` / `2026-12-31` (melingkupi seluruh data) |
| `limit` | `1000` (`100000` untuk `savings.activity_list`) |
| `office_name` | `NULL`, dan putaran kedua `'Branch 001'` |
| `search` | `'a'` |
| `client_id` | `65` (Jasmin Dao — klien dengan transaksi terbanyak) |
| `account_number` | `'Branch 001000000001'` |
| `charge_name` | `'Withdrawal fee'` |
| `currency_code` / `product_ids` | `NULL` (sengaja: menguji perilaku bawaan) |
| `as_of_date` | `2026-09-15` |

## Ringkasan

| Verdict | Jumlah |
| --- | --- |
| lulus | 39 |
| gagal | 9 |
| belum diperiksa | 0 |

48 dari 48 kapabilitas dijalankan ujung ke ujung dan diadu dengan SQL langsung.

**Sembilan yang gagal**, dengan sebabnya:

| Kapabilitas | Sebab |
| --- | --- |
| `organization_office_activity_ranking` | mencampur mata uang (#14) |
| `savings_deposit_total` | mencampur mata uang (#14) |
| `savings_withdrawal_total` | mencampur mata uang (#14) |
| `savings_balance_summary` | mencampur mata uang (#14) |
| `savings_deposit_monthly_breakdown` | mencampur mata uang (#14) |
| `savings_withdrawal_monthly_breakdown` | mencampur mata uang (#14) |
| `client_name_lookup` | `LIMIT 20` keras, memotong hasil diam-diam |
| `savings_client_activity` | `LIMIT 100` keras, memotong hasil diam-diam |
| `savings_charge_type_identity_resolve` | grain resolver ganda untuk entity yang sama — **diperbaiki L1.3 (24 = 24)** |

`organization_office_summary` **gagal saat diukur pertama kali** (fan-out
`m_staff`: 27 kantor untuk 8 kantor nyata) dan kini `lulus` setelah SQL-nya
diperbaiki; lihat §"Perbaikan yang dilakukan".

---

## 1. Organization

### organization_office_summary — **lulus (setelah perbaikan)**

- Kapabilitas **sebelum** perbaikan: `office_count = 27`, `active_staff_count = 24`
- Kapabilitas **sesudah** perbaikan: `office_count = 8`, `root_office_count = 1`,
  `oldest_opening_date = 2009-01-01`, `active_staff_count = 24`
- SQL langsung: `8`, `1`, `2009-01-01`, `24`

```sql
SELECT (SELECT count(*) FROM m_office)                                  AS office_count,
       (SELECT count(*) FROM m_office WHERE parent_id IS NULL)          AS root_office_count,
       (SELECT min(opening_date) FROM m_office)                         AS oldest,
       (SELECT count(*) FROM m_staff WHERE is_active)                   AS active_staff;
```

Sebab kegagalan awal: `LEFT JOIN m_staff` menggandakan baris kantor sebanyak
jumlah stafnya, sehingga `COUNT(o.id)` menghitung pasangan (kantor, staf).
Dengan 25 staf di 6 kantor + 2 kantor tanpa staf → 27. Ini persis kelas cacat
yang `PREPARE` tidak pernah bisa menangkap.

### organization_hierarchy_summary — lulus

- Kapabilitas: `8 / 1 / 7 / 1` (total, root, leaf, max_depth)
- SQL langsung: `8 / 1 / 7 / 1`

```sql
SELECT count(*) AS total,
       count(*) FILTER (WHERE parent_id IS NULL) AS root,
       count(*) FILTER (WHERE NOT EXISTS (SELECT 1 FROM m_office c WHERE c.parent_id=o.id)) AS leaf,
       max(array_length(string_to_array(trim(both '.' from hierarchy),'.'),1)) AS depth
FROM m_office o;
```

`max_hierarchy_depth = 1` berarti "tingkat di bawah root", bukan jumlah tingkat.

### office_list_basic — lulus

- Kapabilitas: 8 baris · SQL langsung: `SELECT count(*) FROM m_office;` → 8

### organization_office_hierarchy_tree — lulus

- Kapabilitas: 8 baris, `depth` 1 untuk Head Office dan 2 untuk tujuh cabang
- SQL langsung: `SELECT count(*) FROM m_office;` → 8; semua cabang `parent_id = 1`

### organization_office_identity_resolve — lulus

- Kapabilitas: 8 baris · SQL langsung: 8 (`m_office`)

### organization_office_name_lookup — lulus

- Kapabilitas (`office_name='Branch 001'`): `1 / 0 / 2025-11-01 / 4`
- SQL langsung: `1 / 0 / 2025-11-01 / 4`

```sql
SELECT count(*), count(*) FILTER (WHERE parent_id IS NULL), min(opening_date),
       (SELECT count(*) FROM m_staff s WHERE s.office_id=2 AND s.is_active)
FROM m_office WHERE lower(name)=lower('Branch 001');
```

Bentuk subquery berkorelasi di sini benar — dan itulah bentuk yang dipakai untuk
memperbaiki `organization_office_summary`.

### organization_office_client_summary — lulus

- Kapabilitas: 8 baris, jumlah `total_clients` = 43, termasuk kantor 9 dengan 0 klien
- SQL langsung: 8 kantor, total 43 klien

```sql
SELECT o.id, count(c.id) FROM m_office o LEFT JOIN m_client c ON c.office_id=o.id
GROUP BY 1 ORDER BY 1;
```

Prosa "including offices without clients" terbukti benar (kantor 9 muncul).

### organization_office_opening_monthly_breakdown — lulus

- Kapabilitas: `2025-11 → 6`, `2026-02 → 1`; total 7
- SQL langsung: 7 kantor dibuka dalam rentang; Head Office (2009-01-01) di luar rentang

```sql
SELECT date_trunc('month',opening_date)::date, count(*) FROM m_office
WHERE opening_date BETWEEN '2025-01-01' AND '2026-12-31' GROUP BY 1 ORDER BY 1;
```

### organization_office_dormant — lulus

- Kapabilitas: 1 baris (kantor 9 "Nour Office", 0 transaksi)
- SQL langsung: 7 kantor punya transaksi dalam rentang → 8 − 7 = 1 dorman

```sql
SELECT office_id, count(*) FROM m_savings_account_transaction
WHERE NOT is_reversed AND transaction_date BETWEEN '2025-01-01' AND '2026-12-31'
GROUP BY 1;   -- 3,4,5,2,6,8,1 → kantor 9 tidak muncul
```

### organization_office_activity_ranking — **gagal**

- Kapabilitas `transaction_count`: `3→5475, 4→2106, 5→466, 2→420, 6→169, 8→127, 1→56`
- SQL langsung: **identik**
- Kapabilitas `deposit_total` kantor 3: `276174.00` · SQL langsung: `276174.00`

```sql
SELECT office_id, count(*),
       sum(CASE WHEN transaction_type_enum=1 THEN amount ELSE 0 END) AS dep,
       sum(CASE WHEN transaction_type_enum=2 THEN amount ELSE 0 END) AS wd
FROM m_savings_account_transaction
WHERE NOT is_reversed AND transaction_date BETWEEN '2025-01-01' AND '2026-12-31'
  AND office_id = ANY(ARRAY[1,2,3,4,5,6,8,9]) GROUP BY 1;
```

Angkanya cocok, **tetapi artinya tidak sah**: `deposit_total` dan
`withdrawal_total` menjumlahkan AED, EUR dan USD menjadi satu angka tanpa
`exchange_rates` dan tanpa `exchange_rate_id` (keputusan #14). Berbeda dari
kapabilitas savings lainnya, query ini bahkan **tidak punya parameter
`currency_code`**, jadi pencampuran tidak dapat dihindari oleh pemanggil.
Sebaran nyata: AED 421.410,00 · EUR 78.224,45 · USD 228.892,00 untuk setoran.

### organization_office_savings_summary — lulus

- Kapabilitas: 20 baris (kantor × mata uang); kantor 3/AED = `55 aktif, 59 total, 143273.55`
- SQL langsung: 20 kelompok (kantor, mata uang)

```sql
SELECT count(*) FROM (
  SELECT o.id, sa.currency_code FROM m_office o
  LEFT JOIN m_client c ON c.office_id=o.id
  LEFT JOIN m_savings_account sa ON sa.client_id=c.id
  GROUP BY 1,2 HAVING count(sa.id)>0) x;   -- 20
```

Query ini `GROUP BY ... sa.currency_code`, jadi tidak mencampur mata uang.
Prosanya diperbaiki agar menyatakan grain itu (lihat §Perbaikan).

---

## 2. Client

### client_lifecycle_summary — lulus

- Kapabilitas: `43 / 38 / 2 / 3` · SQL langsung: `43 / 38 / 2 / 3`

```sql
SELECT count(*), count(*) FILTER (WHERE status_enum=300),
       count(*) FILTER (WHERE status_enum=100), count(*) FILTER (WHERE status_enum=600)
FROM m_client;
```

### client_summary_by_office — lulus

- Kapabilitas: 7 baris, jumlah `total_count` = 43
- SQL langsung: 43 klien tersebar di 7 kantor (kantor 9 tidak punya klien dan,
  karena `JOIN` bukan `LEFT JOIN`, memang tidak muncul — prosanya tidak
  mengklaim sebaliknya)

### client_identity_resolve — lulus

- Kapabilitas: 43 baris · SQL langsung: `SELECT count(*) FROM m_client;` → 43

### clients_with_account_counts — lulus

- Kapabilitas: 43 baris; klien 71 = 3 rekening
- SQL langsung: 43 klien; `SELECT count(*) FROM m_savings_account WHERE client_id=71;` → 3

### client_activation_monthly_breakdown — lulus

- Kapabilitas: `15+4+4+3+7+7 = 40`
- SQL langsung: 40

```sql
SELECT count(*) FROM m_client
WHERE status_enum IN (300,600) AND activation_date BETWEEN '2025-01-01' AND '2026-12-31';
```

### client_activation_top_n_offices — lulus

- Kapabilitas: 7 baris, jumlah = 40 · SQL langsung: 40 (query yang sama)

### client_list_recent — lulus

- Kapabilitas (`office_name=NULL`): 38 baris
- SQL langsung: 38

```sql
SELECT count(*) FROM m_client WHERE status_enum=300 AND activation_date IS NOT NULL;
```

Putaran dengan `office_name='Branch 001'` mengembalikan 4; SQL langsung untuk
kantor 2 juga 4.

### client_random_sample — lulus

- Kapabilitas: 38 baris (limit 1000 > populasi) · SQL langsung: 38 klien aktif

Catatan: `ORDER BY random()` membuat urutannya tidak reprodusibel; yang
diverifikasi adalah himpunannya, bukan urutannya.

### client_name_lookup — **gagal**

- Kapabilitas (`search='a'`): **20 baris**
- SQL langsung: **39 baris**

```sql
SELECT count(*) FROM m_client WHERE display_name ILIKE '%a%';   -- 39
```

`queries/client/name_lookup.sql` memakai `LIMIT 20` yang ditanam di dalam SQL
dan **tidak ada parameter `limit`** di manifes. Pencarian yang luas mengembalikan
daftar terpotong tanpa satu pun sinyal. Prosanya sudah diperbaiki agar
menyatakan batas itu (lihat §Perbaikan), tetapi angkanya tetap tidak sama dengan
SQL langsung, jadi verdict-nya tetap **gagal**: menambah parameter `limit`
mengubah placeholder dan kontrak, dan itu keputusan pemilik repo.

### client_relationship_by_id — lulus

- Kapabilitas (`client_id=65`): 46 baris, `active_savings_account_count = 43`
- SQL langsung: 46 rekening, 43 aktif

```sql
SELECT count(*), count(*) FILTER (WHERE status_enum=300)
FROM m_savings_account WHERE client_id=65;
```

### client_relationship_lookup — lulus

- Kapabilitas (`search='a'`): 210 baris
- SQL langsung: 210

```sql
SELECT count(*) FROM m_client c
LEFT JOIN m_savings_account sa ON sa.client_id=c.id
WHERE c.display_name ILIKE '%a%';
```

Catatan grain: hasilnya berbutir (klien × rekening), bukan satu baris per klien,
dan query ini **tidak punya `LIMIT` sama sekali**. Pada data produksi ia tidak
terbatas. Prosanya sudah menyebut daftar rekening, jadi grain-nya tidak
menyesatkan; batas barisnya yang tidak ada.

### client_savings_overview — lulus

- Kapabilitas (`search='a'`): 39 baris; klien 64 = `4 / 3 / 2 / 40`
- SQL langsung: 39 klien cocok; klien 64 = `4 / 3 / 2 / 40`

```sql
SELECT (SELECT count(*) FROM m_savings_account WHERE client_id=64),
       (SELECT count(*) FROM m_savings_account WHERE client_id=64 AND status_enum=300),
       (SELECT count(*) FROM m_savings_account_charge sac
          JOIN m_savings_account sa ON sa.id=sac.savings_account_id
         WHERE sa.client_id=64 AND sac.is_active AND NOT sac.waived
           AND NOT sac.is_paid_derived AND sac.amount_outstanding_derived>0),
       (SELECT count(*) FROM m_savings_account_transaction t
          JOIN m_savings_account sa ON sa.id=t.savings_account_id
         WHERE sa.client_id=64 AND NOT t.is_reversed);
```

Anti-fanout benar: tiga `LEFT JOIN LATERAL` agregat, bukan join baris.

### client_top_n_by_deposit_volume — lulus

- Kapabilitas: 41 baris; puncak = klien 65 / AED / `134` setoran / `165775.00`
- SQL langsung: 41 kelompok (klien, mata uang); klien 65 / AED = `134` / `165775.00`

```sql
SELECT count(*) FROM (
  SELECT c.id, sa.currency_code FROM m_client c
  JOIN m_savings_account sa ON sa.client_id=c.id
  JOIN m_savings_account_transaction t ON t.savings_account_id=sa.id
  WHERE NOT t.is_reversed AND t.transaction_type_enum=1
    AND t.transaction_date BETWEEN '2025-01-01' AND '2026-12-31'
  GROUP BY 1,2 HAVING sum(t.amount)>0) x;   -- 41
```

### client_top_n_by_savings_account_count — lulus

- Kapabilitas: 28 baris; puncak klien 65 = 43 rekening aktif
- SQL langsung: 28 klien punya ≥1 rekening aktif

```sql
SELECT count(DISTINCT c.id) FROM m_client c
JOIN m_savings_account sa ON sa.client_id=c.id WHERE sa.status_enum=300;   -- 28
```

### client_top_n_by_savings_balance — lulus

- Kapabilitas: 39 baris; puncak klien 65 / AED = `43` rekening / `120602.40`
- SQL langsung: 39 kelompok (klien, mata uang); klien 65 / AED = `43` / `120602.40`

```sql
SELECT count(*), sum(account_balance_derived) FROM m_savings_account
WHERE client_id=65 AND status_enum=300 AND currency_code='AED';   -- 43 | 120602.40
```

---

## 3. Group

### group_identity_resolve — lulus

- Kapabilitas: 2 baris; `member_count` = 3 (Group test) dan 0 (Center 001)
- SQL langsung: 2 grup; anggota 3 dan 0

```sql
SELECT g.id, (SELECT count(*) FROM m_group_client gc WHERE gc.group_id=g.id)
FROM m_group g ORDER BY 1;
```

`LEFT JOIN m_group_client` + `GROUP BY` tidak menggandakan baris grup — grain benar.

---

## 4. Savings

### savings_deposit_total — **gagal**

- Kapabilitas: `deposit_count = 599`, `total_deposit_amount = 728526.45`
- SQL langsung: `599` dan `728526.45` — **dari tiga mata uang**:
  AED `421410.00` (353) + EUR `78224.45` (33) + USD `228892.00` (213)

```sql
SELECT sa.currency_code, count(*), sum(t.amount)
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id=t.savings_account_id
WHERE t.transaction_type_enum=1 AND NOT t.is_reversed
  AND t.transaction_date BETWEEN '2025-01-01' AND '2026-12-31'
  AND t.office_id = ANY(ARRAY[1,2,3,4,5,6,8,9])
GROUP BY ROLLUP(1);
```

Aritmetikanya benar; **semantiknya tidak**. Dengan `currency_code = NULL`
(nilai bawaannya) satu angka dilaporkan untuk tiga mata uang tanpa
`exchange_rates` dan tanpa `exchange_rate_id` — melanggar #14. `deposit_count`
sendiri sah. Entri ini baru dapat lulus bila `currency_code` diwajibkan atau
hasilnya dipecah per mata uang.

### savings_withdrawal_total — **gagal**

- Kapabilitas: `846` / `229417.85`
- SQL langsung: `846` / `229417.85` = AED `143472.10` (402) + EUR `4490.96` (13)
  + USD `81454.79` (431)

SQL sama seperti di atas dengan `transaction_type_enum=2`. Sebab kegagalan
identik (#14); `withdrawal_count` sah.

### savings_balance_summary — **gagal**

- Kapabilitas: `account_count = 169`, `total_balance = 486705.19`,
  `average_balance = 2879.9124`, `max_balance = 30555.43`
- SQL langsung: `169` / `486705.19` = AED `280299.77` (112) + EUR `57573.84` (19)
  + USD `148831.58` (38)

```sql
SELECT sa.currency_code, count(*), sum(sa.account_balance_derived), max(sa.account_balance_derived)
FROM m_savings_account sa JOIN m_client c ON c.id=sa.client_id
WHERE sa.status_enum=300 AND c.office_id = ANY(ARRAY[1,2,3,4,5,6,8,9])
GROUP BY ROLLUP(1);
```

`account_count` sah. `total_balance`, `average_balance` dan `max_balance`
lintas mata uang — `average` dan `max` bahkan lebih menyesatkan daripada `sum`
karena tampak seperti nilai satu rekening.

### savings_deposit_monthly_breakdown — **gagal**

- Kapabilitas: 8 baris bulan; `2025-11 → 63527.00 / 33`, `2026-02 → 124080.00 / 46`
- SQL langsung: jumlah `deposit_count` seluruh bulan = 599 = angka `deposit_total`

Cacahnya cocok; kolom `total_deposit_amount` mencampur AED/EUR/USD (#14) —
sebab yang sama dengan `savings_deposit_total`.

### savings_withdrawal_monthly_breakdown — **gagal**

- Kapabilitas: 8 baris; `2025-11 → 1194.70 / 24`, `2026-02 → 24929.80 / 91`
- SQL langsung: jumlah `withdrawal_count` = 846

Sebab identik (#14).

### savings_deposit_top_n — lulus

- Kapabilitas: 599 baris (limit 1000), `amount` teratas `28000.00`
- SQL langsung: 599 setoran; `max(amount)` = `28000.00`

```sql
SELECT count(*), max(amount) FROM m_savings_account_transaction
WHERE transaction_type_enum=1 AND NOT is_reversed
  AND transaction_date BETWEEN '2025-01-01' AND '2026-12-31'
  AND office_id = ANY(ARRAY[1,2,3,4,5,6,8,9]);
```

Tidak ada pencampuran mata uang: hasilnya per transaksi dan `currency_code`
ikut sebagai kolom.

### savings_withdrawal_top_n — lulus

- Kapabilitas: 846 baris, `amount` teratas `5630.00`
- SQL langsung: 846; `max(amount)` = `5630.00`

### savings_deposit_monthly_top_n — lulus

- Kapabilitas (`limit=1000` per bulan): 599 baris
- SQL langsung: 599 setoran dalam rentang → seluruh baris terambil, jadi
  `ROW_NUMBER()` per bulan tidak menghilangkan atau menggandakan satu pun baris

### savings_withdrawal_monthly_top_n — lulus

- Kapabilitas: 846 baris · SQL langsung: 846 penarikan

### savings_activity_list — lulus

- Kapabilitas (`limit=100000`): `8819` baris
- SQL langsung: `8819`

```sql
SELECT count(*) FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id=t.savings_account_id
WHERE NOT t.is_reversed AND t.transaction_date BETWEEN '2025-01-01' AND '2026-12-31'
  AND t.office_id = ANY(ARRAY[1,2,3,4,5,6,8,9]);
```

`JOIN m_savings_product` dan `JOIN m_office` bersifat many-to-one, `LEFT JOIN
m_client` juga — tidak ada fan-out: 8819 baris untuk 8819 transaksi.

### savings_client_activity — **gagal**

- Kapabilitas (`client_id=65`): **100 baris**
- SQL langsung: **4114 baris**

```sql
SELECT count(*) FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id=t.savings_account_id
WHERE NOT t.is_reversed AND sa.client_id=65
  AND t.office_id = ANY(ARRAY[1,2,3,4,5,6,8,9]);   -- 4114
```

`LIMIT 100` ditanam di dalam SQL, tanpa parameter `limit` di manifes. Untuk
klien aktif, 97,6% riwayatnya hilang tanpa sinyal. Prosanya sudah diperbaiki
agar menyatakan batas itu; verdict tetap **gagal** dengan alasan yang sama
seperti `client_name_lookup`.

### savings_account_charges_recent — lulus

- Kapabilitas: 182 baris · SQL langsung: 182

```sql
SELECT count(*) FROM m_savings_account_charge sac
JOIN m_savings_account sa ON sa.id=sac.savings_account_id
JOIN m_client c ON c.id=sa.client_id
WHERE c.office_id = ANY(ARRAY[1,2,3,4,5,6,8,9]);   -- 182
```

`LEFT JOIN LATERAL ... LIMIT 1` untuk `m_organisation_currency` tidak
menggandakan baris — grain satu baris per charge terjaga.

### savings_charges_by_type — lulus

- Kapabilitas (`charge_name='Withdrawal fee'`): 33 baris
- SQL langsung: 33

### savings_charge_count_by_type — lulus

- Kapabilitas: `charge_count = 33`, `savings_account_count = 33`,
  `amount_outstanding_total = -5.78`
- SQL langsung: `33` / `33` / `-5.78`

```sql
SELECT count(*), count(DISTINCT sac.savings_account_id), sum(sac.amount_outstanding_derived)
FROM m_savings_account_charge sac
JOIN m_savings_account sa ON sa.id=sac.savings_account_id
JOIN m_client c ON c.id=sa.client_id
JOIN m_charge ch ON ch.id=sac.charge_id
WHERE sac.is_active AND lower(ch.name)='withdrawal fee'
  AND c.office_id = ANY(ARRAY[1,2,3,4,5,6,8,9]);
```

Nilai negatif `-5.78` bukan cacat query: `amount_outstanding_derived` memang
negatif pada beberapa charge di data ini (dibayar lebih).
**Risiko mata uang yang belum terbukti:** `amount_outstanding_total`
menjumlahkan tanpa memandang `sa.currency_code`. Pada data ini tidak ada satu
pun nama charge yang menjangkau lebih dari satu mata uang —
`GROUP BY ch.name HAVING count(DISTINCT sa.currency_code)>1` mengembalikan 0
baris — jadi pencampuran tidak terjadi di sini. Ia tidak dicegah oleh apa pun.

### savings_charge_type_identity_resolve — **lulus (diperbaiki L1.3 / FIN-34)**

- Kapabilitas: **24 baris**
- SQL langsung: **24** definisi charge yang berbeda

```sql
-- setelah perbaikan: charge_time_enum dikeluarkan dari SELECT DISTINCT
SELECT count(*), count(DISTINCT charge_definition_id) FROM (
  SELECT DISTINCT ch.id AS charge_definition_id, ch.name, ch.currency_code, ch.is_penalty
  FROM m_savings_account_charge sac
  JOIN m_savings_account sa ON sa.id=sac.savings_account_id
  JOIN m_client c ON c.id=sa.client_id
  JOIN m_charge ch ON ch.id=sac.charge_id
  WHERE c.office_id = ANY(ARRAY[1,2,3,4,5,6,8,9])
    AND ch.is_active AND NOT ch.is_deleted) x;                 -- 24 baris, 24 id
```

`SELECT DISTINCT` dulu menyertakan `sac.charge_time_enum`, yang berasal dari
*penerapan* charge, bukan dari definisinya. Definisi 20 dan 22 dipasang dengan
dua `charge_time_enum` berbeda, sehingga resolver menampilkan **dua kandidat
dengan label identik untuk entity yang sama**. Pada resolver identitas ini fatal:
pengguna dihadapkan pada dua pilihan yang tidak dapat dibedakan, dan
`resolver_unique` tidak akan pernah terpicu untuk kedua charge itu.
Perbaikan (L1.3): `charge_time_enum` dikeluarkan dari source SQL, fragment, dan
`output_fields` shape/query. Grain resolver = grain definisi; 24 baris = 24 id
(sebelumnya 26). Perubahan `output_fields`/proyeksi dataset dilakukan sebagai
commit owner (Rule 3).

### savings_pending_charges_clients — lulus

- Kapabilitas (`as_of_date='2026-09-15'`): 82 baris
- SQL langsung: 82

```sql
SELECT count(*) FROM m_savings_account_charge sac
JOIN m_savings_account sa ON sa.id=sac.savings_account_id
JOIN m_client c ON c.id=sa.client_id
WHERE NOT sac.waived AND NOT sac.is_paid_derived AND sac.is_active
  AND sac.amount_outstanding_derived>0
  AND c.office_id = ANY(ARRAY[1,2,3,4,5,6,8,9]);   -- 82
```

Semantik as-of benar dan sesuai prosa: `as_of_date` **hanya** dipakai untuk
menghitung `days_overdue`, tidak menyaring baris.

### savings_strictly_overdue_charges_clients — lulus

- Kapabilitas (`as_of_date='2026-09-15'`): 76 baris
- SQL langsung: 76 (query di atas + `AND sac.charge_due_date < DATE '2026-09-15'`)

82 − 76 = 6 charge outstanding tanpa tanggal jatuh tempo atau jatuh tempo di
masa depan — persis yang prosanya nyatakan sengaja dikecualikan.

### savings_products_by_client — lulus

- Kapabilitas (`client_id=65`): 3 baris
- SQL langsung: `SELECT count(DISTINCT product_id) FROM m_savings_account WHERE client_id=65;` → 3

### savings_deposit_maturity_by_client — lulus

- Kapabilitas (`client_id=65`): 1 baris (rekening 137, jatuh tempo 2026-10-01)
- SQL langsung: 1

```sql
SELECT count(*) FROM m_savings_account
WHERE client_id=65 AND deposit_type_enum IN (200,300);   -- 1
```

Catatan: `LIMIT 100` keras seperti pada `savings_client_activity`, tetapi pada
data ini batas itu tidak pernah tersentuh, jadi kedua angkanya cocok.

### savings_account_identity_lookup — lulus

- Kapabilitas (`account_number='Branch 001000000001'`): 1 baris, `****0001`,
  klien 1, kantor 2, produk 1, status 300, USD
- SQL langsung: 1 baris dengan nilai yang sama

```sql
SELECT sa.id, sa.client_id, c.office_id, sa.product_id, sa.status_enum, sa.currency_code
FROM m_savings_account sa JOIN m_client c ON c.id=sa.client_id
WHERE sa.account_no='Branch 001000000001';
```

### savings_account_terms_lookup — lulus

- Kapabilitas: 1 baris; bunga rekening `2.000000`, bunga produk `2.000000`,
  overdraft rekening `0.000000`, overdraft produk `NULL`
- SQL langsung: nilai yang sama dari `m_savings_account` dan `m_savings_product`

---

## 5. Butir yang queries/CARRY-OVER.md sebut belum pernah diperiksa

### 5.1 Dua kelas timeout

Seluruh 48 query dijalankan dengan `\timing`. Waktu eksekusi terukur:

| | Waktu |
| --- | --- |
| Tercepat | 0,8 ms (`organization.hierarchy_summary`) |
| Terlambat | 45,9 ms (`client.savings_overview`) |
| Berikutnya | 16,4 ms (`savings.activity_list`), 13,1 ms (`savings.withdrawal_monthly_top_n`) |

**Pada data ini tidak satu pun query mendekati `PROBE_QUERY_TIMEOUT_MS` (3000 ms).**
Itu bukan bukti bahwa mereka aman pada data produksi: basis ini hanya punya
15.607 transaksi dan 43 klien. `client.savings_overview` (tiga `LATERAL` per
klien) dan `savings.*_monthly_top_n` (window function di atas seluruh rentang)
adalah kandidat pertama yang akan melewati 3 detik ketika data tumbuh.
`timeout_ms` yang dideklarasikan tiap manifes (3000/5000/8000) **belum**
dipetakan ke dua kelas `PROBE` vs `ANALYTICAL`; nilai 5000 dan 8000 saat ini
tidak cocok dengan kelas mana pun. Itu keputusan kontrak, tidak diubah di sini.

### 5.2 Grain dan anti-fanout

Diperiksa dengan membandingkan cacah baris hasil terhadap cacah entitas:

| Temuan | Status |
| --- | --- |
| `organization.office_summary` — `LEFT JOIN m_staff` menggandakan kantor (27 vs 8) | **ditemukan dan diperbaiki** |
| `savings.charge_definitions.source` — 26 baris untuk 24 definisi | **ditemukan, diperbaiki (L1.3): 24 = 24** |
| `client.savings_overview` — tiga `LEFT JOIN LATERAL` agregat | aman |
| `savings.account_charges_recent`, `charges_by_type`, `pending/overdue` — `LATERAL … LIMIT 1` untuk simbol mata uang | aman |
| `savings.activity_list` / `*_top_n` — join many-to-one ke produk/kantor/klien | aman (8819 = 8819) |
| `client.relationship_lookup` / `relationship_by_id` | berbutir klien × rekening **secara sengaja**; prosanya menyatakannya |
| `client.top_n_by_deposit_volume`, `top_n_by_savings_balance`, `organization.office_savings_summary` | berbutir × mata uang; prosanya **tidak** menyatakannya → diperbaiki |
| `group.groups.source` — `LEFT JOIN m_group_client` + `GROUP BY` | aman |

`datasets.grain_json` (#11) sendiri **belum** diisi untuk satu dataset pun; ini
mencatat grain nyatanya, bukan mendeklarasikannya di tempat yang diminta #11.

### 5.3 Penamaan kolom terhadap aturan validasi response

[contracts/responses.md](../docs/contracts/responses.md) mengikat satu hal
tentang kolom: bila PII dimatikan, kolom berkelas `pii` tidak boleh muncul di
blok `table` (baris 175, `RESP-8.8`). Itu dapat dipenuhi per kapabilitas dan
tidak dilanggar oleh satu pun manifes: tidak ada `never_return`
(`account_no`, `external_id`, `mobile_no`, `email_address`) yang muncul sebagai
`output_fields` di mana pun. Yang ada hanya `masked_account_number`
(`masked_output`).

Tidak ada aturan penamaan lain yang tertulis di dokumen itu. Satu
**ketidakkonsistenan** tetap dicatat tanpa diperbaiki, karena memperbaikinya
berarti mengarang aturan yang tidak ada (build-order §1 Aturan 4):
empat manifes memakai `display_name` (`client.name_lookup`,
`client.relationship_by_id`, `client.relationship_lookup`,
`client.savings_overview`) sementara 20 manifes lain memakai
`client_display_name` untuk kolom yang persis sama.

### 5.4 Semantik mata uang

Data ini memakai **tiga** mata uang: AED (127 rekening), USD (46), EUR (34).
Karena itu pertanyaan ini dapat diuji, bukan diteorikan.

| Kapabilitas | Perilaku |
| --- | --- |
| `savings_deposit_total`, `savings_withdrawal_total`, `savings_balance_summary`, `savings_deposit_monthly_breakdown`, `savings_withdrawal_monthly_breakdown` | menjumlahkan lintas mata uang bila `currency_code = NULL` → **gagal** (#14) |
| `organization_office_activity_ranking` | menjumlahkan lintas mata uang dan **tidak punya parameter `currency_code`** → **gagal**, paling berat |
| `savings_charge_count_by_type` | tidak punya penjaga mata uang; pada data ini tidak ada charge lintas mata uang, jadi tidak terbukti salah |
| `client_top_n_by_deposit_volume`, `client_top_n_by_savings_balance`, `organization_office_savings_summary` | `GROUP BY … currency_code` → aman |
| `savings_deposit_top_n`, `savings_withdrawal_top_n`, `*_monthly_top_n`, `savings_activity_list` | per transaksi, `currency_code` sebagai kolom → aman |

Tidak satu pun query memakai `exchange_rates` atau merekam `exchange_rate_id`,
dan tidak satu pun menanam kurs di dalam SQL. Larangan kedua dipatuhi;
kewajiban pertama belum dipenuhi oleh enam kapabilitas di atas.

---

## 6. Aturan 3 — kelas sensitivitas

Seluruh 111 `output_fields` memakai kelas yang terdaftar di
[policies/pii.yaml](policies/pii.yaml) dan
[schema/fineract/columns/sensitivity.yaml](schema/fineract/columns/sensitivity.yaml).
Sebarannya: `public_business` untuk hampir semuanya, `pii` untuk
`client_display_name` dan `display_name`, `masked_output` untuk
`masked_account_number`.

Satu **ketidakkonsistenan nyata** ditemukan dan diperbaiki: `client_id`
dideklarasikan `pii` di 4 manifes dan `public_business` di 20 manifes lain —
kolom yang sama, dua kelas. Lihat §Perbaikan untuk arah yang dipilih dan
alasannya.

## 7. Aturan 4 — semantik cutoff / as-of

- 17 kapabilitas terikat rentang tanggal (`from_date`/`to_date`) → cutoff
  dideklarasikan lewat parameter.
- 2 kapabilitas charge memakai `as_of_date`, dan semantiknya diuji di atas
  (82 vs 76 baris) → dideklarasikan dan terbukti.
- Sisanya bersifat snapshot. Hanya 7 di antaranya yang mendeklarasikan
  `guards.snapshot_only`; **24 tidak mendeklarasikan apa pun**. Semuanya kini
  mendeklarasikannya (lihat §Perbaikan). Perlu dicatat dengan jujur:
  `snapshot_only` saat ini **tidak ditegakkan oleh validator mana pun** —
  ia deklarasi, bukan mekanisme.

---

## 8. Perbaikan yang dilakukan

Hanya file di bawah `knowledge/` dan `queries/` yang disentuh.
`cargo run -p app -- catalog` sesudahnya: **0 error, 4 warning (keempatnya sudah
ada sebelum pekerjaan ini), 48 capability, status katalog: validated**.

1. **`queries/organization/office_summary.sql`** — `LEFT JOIN m_staff` diganti
   subquery berkorelasi. `office_count` 27 → 8. Ini satu-satunya perubahan
   perilaku query; dibuktikan oleh dua angka di §1.

2. **Kelas sensitivitas `client_id` diseragamkan menjadi `public_business`**
   pada `knowledge/queries/client/relationship_by_id.yaml`,
   `relationship_lookup.yaml`, `savings_overview.yaml`, dan
   `knowledge/queries/savings/account_identity_lookup.yaml`.
   Alasan: `sensitivity.yaml` mendefinisikan `pii` sebagai pengenal personal
   langsung (`display_name`, `mobile_no`, `email_address`); `client_id` adalah
   kunci pengganti, dan 20 manifes lain — termasuk seluruh resolver identitas —
   sudah mengklasifikasikannya `public_business`. Menaikkan 20 manifes menjadi
   `pii` justru akan membuat resolver identitas mustahil dipakai saat PII mati,
   karena tidak ada id yang dapat dipilih.
   **Ini keputusan kebijakan, bukan temuan mekanis.** Bila pemilik repo memilih
   arah sebaliknya, empat file inilah yang perlu dikembalikan.

3. **`guards.snapshot_only: true` ditambahkan** pada 24 kapabilitas snapshot
   yang tidak mendeklarasikan semantik as-of apa pun:
   `clients_with_account_counts`, `client_identity_resolve`,
   `client_name_lookup`, `client_relationship_by_id`,
   `client_relationship_lookup`, `client_savings_overview`, `client_list_recent`,
   `client_random_sample`, `client_top_n_by_savings_account_count`,
   `client_top_n_by_savings_balance`, `group_identity_resolve`,
   `organization_office_identity_resolve`, `office_list_basic`,
   `organization_office_hierarchy_tree`, `organization_office_client_summary`,
   `organization_office_savings_summary`, `savings_account_charges_recent`,
   `savings_account_identity_lookup`, `savings_account_terms_lookup`,
   `savings_charge_type_identity_resolve`, `savings_charges_by_type`,
   `savings_client_activity`, `savings_deposit_maturity_by_client`,
   `savings_products_by_client`.

4. **Prosa diperbaiki agar sesuai SQL-nya** (Aturan 2):
   - `client_top_n_by_deposit_volume`, `client_top_n_by_savings_balance`,
     `organization_office_savings_summary` — grain per mata uang kini
     dinyatakan; sebelumnya prosanya berbunyi "per klien"/"per kantor"
     padahal hasilnya 41 baris untuk 28 klien dan 20 baris untuk 8 kantor.
   - `client_name_lookup` — `LIMIT 20` keras kini dinyatakan.
   - `savings_client_activity` — `LIMIT 100` keras kini dinyatakan.

## 9. Yang sengaja **tidak** dikerjakan

- Tidak ada perbaikan untuk enam kapabilitas yang mencampur mata uang:
  memperbaikinya berarti mewajibkan `currency_code` atau memecah hasil per mata
  uang — keduanya mengubah kontrak `output_fields`/parameter.
- Tidak ada parameter `limit` yang ditambahkan ke `client.name_lookup` atau
  `savings.activity_by_client`: menambah placeholder mengubah kontrak manifes.
- `savings.charge_definitions.source` tidak diperbaiki: mengeluarkan
  `charge_time_enum` mengubah `output_fields` dan proyeksi dataset.
- `datasets.grain_json` (#11) tidak diisi.
- Tidak ada dokumen di bawah `docs/` yang disentuh, dan aturan di kedua
  `CARRY-OVER.md` tidak ditulis ulang.
- Angka di sini diukur pada **satu** basis data lokal berukuran kecil. Kelas
  cacat yang hanya muncul pada volume produksi — terutama timeout — tetap
  belum terbukti ke arah mana pun.
