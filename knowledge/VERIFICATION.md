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
| lulus | 48 |
| gagal | 0 |
| belum diperiksa | 0 |

48 dari 48 kapabilitas dijalankan ujung ke ujung dan diadu dengan SQL langsung.
Sembilan yang semula gagal kini **semua diperbaiki** (L1.1–L1.3, ditutup gerbang
L1.4/FIN-35): keempat aturan CARRY-OVER berlaku untuk 48 kapabilitas dan validator
katalog menegakkan grain, kelas timeout, penamaan kolom, dan bentuk output secara
mekanis (`cargo run -p app -- catalog` → 0 error).

**Sembilan yang semula gagal** (kini diperbaiki), dengan sebabnya:

| Kapabilitas | Sebab |
| --- | --- |
| `organization_office_activity_ranking` | mencampur mata uang (#14) — **diperbaiki L1.1 (pisah per mata uang)** |
| `savings_deposit_total` | mencampur mata uang (#14) — **diperbaiki L1.1 (pisah per mata uang)** |
| `savings_withdrawal_total` | mencampur mata uang (#14) — **diperbaiki L1.1 (pisah per mata uang)** |
| `savings_balance_summary` | mencampur mata uang (#14) — **diperbaiki L1.1 (pisah per mata uang)** |
| `savings_deposit_monthly_breakdown` | mencampur mata uang (#14) — **diperbaiki L1.1 (pisah per mata uang)** |
| `savings_withdrawal_monthly_breakdown` | mencampur mata uang (#14) — **diperbaiki L1.1 (pisah per mata uang)** |
| `client_name_lookup` | `LIMIT 20` keras, memotong hasil diam-diam — **diperbaiki L1.2**: cap dibuang, populasi penuh (39 = 39) |
| `savings_client_activity` | `LIMIT 100` keras, memotong hasil diam-diam — **diperbaiki L1.2**: `LIMIT` jadi parameter `limit` ber-`max_limit` (bound & dideklarasikan), total sebenarnya 4114 diungkap lewat limitation |
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

### organization_office_activity_ranking — **lulus (diperbaiki L1.1)**

- Kapabilitas `transaction_count` (total lintas mata uang): `3→5475, 4→2106, 5→466, 2→420, 6→169, 8→127, 1→56`
- SQL langsung: **identik**
- **Perbaikan L1.1**: output kini dipecah per mata uang (`GROUP BY … sa.currency_code`
  setelah JOIN `m_savings_account`). Kantor 3: AED 5135 · EUR 32 · USD 308 = 5475 —
  JOIN tidak menggandakan hitungan. Tidak ada lagi `deposit_total`/`withdrawal_total`
  yang menjumlahkan AED+EUR+USD jadi satu angka.

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

### savings_account_identity_lookup — lulus (FIN-137: account_number sekarang identity slot beresolver)

- Kapabilitas (`savings_account_id=1`, resolved dari account_number lewat
  `savings_account_identity_resolve`): 1 baris, `****0001`, klien 1, kantor 2,
  produk 1, status 300, USD
- SQL langsung: 1 baris dengan nilai yang sama

```sql
SELECT sa.id, sa.client_id, c.office_id, sa.product_id, sa.status_enum, sa.currency_code
FROM m_savings_account sa JOIN m_client c ON c.id=sa.client_id
WHERE sa.id=1;   -- account_no='Branch 001000000001'
```

Sebelum FIN-137, `account_number` dideklarasikan query manifest sebagai
`source: transient_sensitive_input` tanpa `probe:` capability — K1
(`planner.rs Missing::unanswerable`) membuat slot itu **tidak dapat
ditanyakan sama sekali**, jadi capability ini selalu `Unsupported` /
`identity_slot_without_resolver` (dibuktikan `resolver/noresolver-*.yml`).
Perbaikan: query & capability sekarang mengikat `savings_account_id`
(integer, bukan lagi `account_number` string), diisi lewat probe baru
`savings_account_identity_resolve` (dataset `savings.accounts`, shape
`identity_candidates`) — masked label + savings_account_id saja, nomor
rekening mentah tidak pernah diproyeksikan (`sensitive_business_identifier`,
`columns/sensitivity.yaml`).

### savings_account_terms_lookup — lulus (FIN-137: sama seperti di atas)

- Kapabilitas (`savings_account_id=2`, `****0002`): bunga rekening
  `2.000000`, bunga produk `2.000000`, overdraft rekening `false`/`0.000000`,
  overdraft produk `false`/`NULL`
- SQL langsung: nilai yang sama dari `m_savings_account` dan `m_savings_product`

### savings_account_identity_resolve — lulus (probe baru, FIN-137)

- Kapabilitas (`office_ids` = seluruh 7 office / scope admin): 205 baris,
  205 `savings_account_id` distinct — tidak ada fanout dari join
  `m_client`/`m_office`/`m_savings_product`.
- SQL langsung: 205 baris, 205 id.

```sql
SELECT count(*), count(DISTINCT sa.id) FROM m_savings_account sa
JOIN m_client c ON c.id=sa.client_id
JOIN m_office o ON o.id=c.office_id
JOIN m_savings_product sp ON sp.id=sa.product_id;   -- 205, 205
```

`right(account_no,4)` unik untuk rekening 1 dan 2 (`'0001'`/`'0002'`) di
seluruh 205 baris — `q=0001` pada `/clarification/options` mempersempit ke
tepat satu kandidat tanpa memperluas scope (CLR-7), dibuktikan
`account-identity-lookup-options.yml`/`account-terms-lookup-options.yml`.
Tidak ada office dengan tepat satu rekening pada data ini (minimum 6, office
8), jadi K5 `resolver_unique` auto-bind untuk slot ini tidak dibuktikan ulang
di sini dengan fixture segar — mekanismenya generik dan sama dengan yang
`resolver/autobind-*.yml` sudah buktikan untuk `client_id`.

### savings_charge_count_by_type / savings_charges_by_type — charge_name diikat ke probe (FIN-137)

`charge_name` sekarang mendeklarasikan `probe:` ke
`savings_charge_type_identity_resolve` (dataset `savings.charge_definitions`,
shape `charge_type_candidates`, `output_slot: charge_name`) — keduanya
memfilter `ch.name`/`m_charge.name` case-insensitively, kolom yang sama yang
probe resolusi (`charge_name`), jadi nilai yang di-resolve mengikat persis
seperti teks yang diketik akan mengikat. Angka jawaban tidak berubah
(`tests/answers/savings_charge_count_by_type.sql`,
`savings_charges_by_type.sql` tetap `charge_name: "Withdrawal fee"` — hanya
mekanisme pengikatannya yang berubah dari teks bebas menjadi opsi resolver).

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

**FIN-36 (L1.5): grain kini dideklarasikan eksplisit (#11).** Tiap manifest di
`knowledge/queries/**/*.yaml` menyatakan `grain: [kolom, …]` — himpunan minimal
kolom `output_fields` yang mengidentifikasi satu baris hasil — dan validator
katalog menegakkan dua hal: grain tidak boleh kosong (`grain_declared`) dan tiap
kolom grain wajib salah satu `output_fields` (`grain_subset_of_output`).

Untuk tujuh query yang §5.2 tandai rawan fanout/×mata uang, grain yang
dideklarasikan diverifikasi ke database: query dijalankan atas scope
`ARRAY[1,2,3,4,5,6,8,9]` (himpunan P: from/to 2025-01-01/2026-12-31, client_id
65, search 'a', limit 1000; `activity_list` limit 100000) dan `row_count`
dibandingkan dengan `COUNT(DISTINCT <kolom grain>)` — keduanya harus sama:

| Query | grain dideklarasikan | row_count = COUNT(DISTINCT grain) |
| --- | --- | --- |
| `client.relationship_lookup` | `[client_id, savings_account_id]` | 210 = 210 |
| `client.relationship_by_id` | `[client_id, savings_account_id]` | 46 = 46 |
| `client.top_n_by_deposit_volume` | `[client_id, currency_code]` | 41 = 41 |
| `client.top_n_by_savings_balance` | `[client_id, currency_code]` | 39 = 39 |
| `organization.office_savings_summary` | `[office_id, currency_code]` | 20 = 20 |
| `savings.activity_list` | `[transaction_id]` | 8819 = 8819 |
| `organization.office_activity_ranking` | `[office_id, currency_code]` | 20 = 20 |

Angka kiri = kanan pada ketujuhnya: grain klien×rekening dan ×mata uang yang
sengaja itu terbukti benar-benar unik per baris, bukan fanout tak terdeklarasi.

Catatan cakupan: validator membuktikan grain **dinyatakan** dan **subset**
`output_fields`, bukan bahwa grain itu benar-benar grain hasil sebenarnya untuk
tiap query (itu dibuktikan dengan menjalankan query, seperti tabel di atas untuk
tujuh yang rawan).

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
| `savings_deposit_total`, `savings_withdrawal_total`, `savings_balance_summary`, `savings_deposit_monthly_breakdown`, `savings_withdrawal_monthly_breakdown` | dulu menjumlahkan lintas mata uang bila `currency_code = NULL` → **diperbaiki L1.1**: `GROUP BY … currency_code`, hasil dipecah per mata uang, tak ada penjumlahan lintas mata uang |
| `organization_office_activity_ranking` | dulu menjumlahkan lintas mata uang tanpa parameter `currency_code` → **diperbaiki L1.1**: JOIN `m_savings_account` + `GROUP BY … currency_code`, output dipecah per mata uang |
| `savings_charge_count_by_type` | tidak punya penjaga mata uang; pada data ini tidak ada charge lintas mata uang, jadi tidak terbukti salah |
| `client_top_n_by_deposit_volume`, `client_top_n_by_savings_balance`, `organization_office_savings_summary` | `GROUP BY … currency_code` → aman |
| `savings_deposit_top_n`, `savings_withdrawal_top_n`, `*_monthly_top_n`, `savings_activity_list` | per transaksi, `currency_code` sebagai kolom → aman |

**Angka per mata uang (L1.1, scope 8 kantor, 2025-01-01…2026-12-31, kapabilitas = SQL langsung):**

| Kapabilitas | AED | EUR | USD |
| --- | --- | --- | --- |
| `savings_deposit_total` | 421.410,00 (353) | 78.224,45 (33) | 228.892,00 (213) |
| `savings_withdrawal_total` | 143.472,10 (402) | 4.490,96 (13) | 81.454,79 (431) |
| `savings_balance_summary` (total, akun aktif) | 280.299,77 (112) | 57.573,84 (19) | 148.831,58 (38) |
| `organization_office_activity_ranking` (kantor 3, txn) | 5.135 | 32 | 308 = **5.475** total |

Tidak satu pun query memakai `exchange_rates` atau merekam `exchange_rate_id`,
dan tidak satu pun menanam kurs di dalam SQL. Larangan kedua tetap dipatuhi;
kewajiban pertama kini dipenuhi dengan **pemisahan per mata uang** (bukan
konsolidasi kurs — sumber kurs masih "Terbuka" per D03). Konsolidasi kurs
tertelusur adalah pekerjaan terpisah saat provider kurs tersedia.

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

## 10. FIN-117 / D08 — group/center meeting schedules

This is a deployment-specific Mode-1 approval, not a claim that calendars
prove attendance. On `fineract_default` (2026-09-28), `m_group` has two rows:
group 1 (level `Group`, office 3, no assigned staff, direct members 4/5/9)
and center 2 (level `Center`, office 2, staff 1, no direct members).
`m_calendar` and `m_calendar_instance` have 35 rows each; exactly two instances
belong to these groups: `(entity_type_enum, entity_id, calendar_id)` is
`(2,1,4)` and `(4,2,3)`. Both calendar rows have `calendar_type_enum=1`,
`repeating=true`, `recurrence='FREQ=DAILY'`, start dates 2025-12-05 and
2025-12-04 respectively, no end date or meeting time. Other instance enums
3/5/6 are not group/center evidence. `m_meeting` and
`m_client_attendance` each contain **zero** rows; actual meeting occurrence and
attendance are not provable here.

The four CARRY-OVER checks for `group_center_meeting_schedule`:

1. **End-to-end numbers.** For the inclusive Monday–Sunday window
   2026-09-28…2026-10-04, bound office 3 and direct SQL independently agree
   on the first two decisive measurements:

   | Measurement | Capability SQL | Independent direct SQL |
   | --- | ---: | ---: |
   | Scheduled group/member rows, office 3 | 21 | 21 |
   | Distinct scheduled dates, office 3 | 7 | 7 |
   | Distinct directly linked members, office 3 | 3 (IDs 4, 5, 9) | 3 (IDs 4, 5, 9) |
   | Scheduled center rows, office 2 | 7 (staff 1, member NULL) | 7 (staff 1, no direct member) |

   The independent count starts from `m_group`, joins the calendar-instance
   enum and meeting-type rows, expands seven dates from the requested week,
   and counts actual `m_group_client` links; it does not count meetings or
   attendance. The independently written single-day row oracle is
   `tests/answers/group_center_meeting_schedule.sql`, consumed by the
   `answers/group/fin117-*` job chain (columns, rows and bound parameters
   compared with direct SQL). The office-narrowed live HTTP week proof is
   in `engine/fin117-meeting-*` and `engine/fin117-center-*`.
2. **Prose equals SQL.** The query returns a date **of a scheduled
   occurrence**, not `m_calendar.start_date` mislabeled as this week's
   meeting. It joins `entity_type_enum=2` only to groups with direct clients
   and `=4` only to centers, requires meeting calendar type 1, and admits the
   two recurrence forms this deployment has verified: nonrepeating start
   dates and `FREQ=DAILY`. No fabricated center descendant roster or staff
   assignment: both unassigned fields are NULL on the relevant row.
3. **PII and identifiers.** `knowledge/schema/fineract/group_center.yaml:20-21`
   classifies `m_group.display_name` as `pii`; it is **not selected**.
   Neither staff/client display names nor group account/external IDs are
   fetched. Group, staff, and client surrogate IDs are
   `public_business`; when a name is required, the missing PII authorization
   is not silently replaced with a name.
4. **Time and evidence.** The SQL binds `from_date`/`to_date` (maximum
   seven-day window) and reports both occurrence date and calendar
   start/end/repeating/recurrence. `this week` for a **schedule** includes
   Monday–Sunday, whereas transactional historical periods still end today.
   No as-of history is claimed: staff assignment and direct membership are
   current snapshots, not membership or attendance at a past event.

Activation on another deployment requires verifying group/center use,
calendar entity/meeting enums, recurrence forms, and PII classifications
first. Without that evidence, leave the area conditional and domain and
capability candidate; the conditional guard returns
`Completed+Unsupported` / `domain_conditional_not_enabled`. Even with
activation, requests for realized meetings or attendance stay
`Completed+Unsupported` / `domain_gap` until source records and a separate
approved capability establish them.

Live HTTP proofs (locked runner, port 3317): activated `engine` folder,
165 requests/202 tests, checked both offices against the independent counts
above and the attendance negative path. An isolated deployment-candidate
catalog (all other knowledge and the database unchanged, only the area,
domain, and schedule capability statuses switched back to
conditional/candidate) passed `params` with 22 requests/29 tests; the
scheduled-group request completed `Unsupported`, `Unknown`,
`domain_conditional_not_enabled`, no plan. The temporary catalog and Bruno
requests were removed; the normal `.env` catalog path was restored before
the full run.

The locked `answers` stage passed 220 requests/272 tests, including
`FIN-52 group_center_meeting_schedule: jawaban job = SQL langsung` and
the `FIN-117 D08` completeness check (54 independently computed
answer-oracle files).

## 11. FIN-108 / LOAN-1..3 — loan identity, lifecycle and terms

Two independent writings per capability, all offices in scope, client 65:

| Capability | Production rows | Oracle rows | Equal |
| --- | --- | --- | --- |
| `loans_by_client` | 24 | 24 (EXISTS scope, `r_enum_value` labels) | yes |
| `loan_status_summary` | 11 | 11 (UNION scope CTE) | yes |
| `loan_terms_by_client` | 24 | 24 (correlated product subqueries) | yes |

Client 65 holds 24 loans in statuses 200/300; 23 of 116 loans locally differ
from their product's current rate or repayment count, so substituting product
defaults would be visibly wrong. The locked `answers` stage passed 310
requests / 389 tests including the three FIN-108 chains.

## 12. FIN-108 slice 2 / LOAN-5, LOAN-6 — loan schedule and transactions

| Capability | Production rows | Oracle rows | Equal |
| --- | --- | --- | --- |
| `loan_schedule_by_client` | 78 (client 14) | 78 (EXISTS scope) | yes |
| `loan_transactions` | 100 of 2,859 (last 12 months, LIMIT 100) | 100 (`:today`-token oracle, no fixed dates) | yes |

Allocation check (`principal + interest + fee + penalty + overpayment`)
reconciles to `amount` for every REPAYMENT-type row in the last-12-months
sample; the 95/2,859 DISBURSEMENT-type mismatches across the whole table are
expected (disbursement is not allocated across those columns) and are
documented in `loans-servicing.yaml`, not treated as a defect. The locked
`answers` stage passed 322 requests / 405 tests including the two new chains.

## 13. FIN-108 slice 3 / LOAN-7, LOAN-8, LOAN-9 — loan charges, balances, arrears

All offices in scope (`office_id IN (1,2,3,4,5,6,8,9)` / `ANY(ARRAY[2,3,5,8,9,4,6,1])`,
both forms independently written):

| Capability | Production rows | Oracle rows | Equal |
| --- | --- | --- | --- |
| `loan_charges` | 217 | 217 (correlated `EXISTS` scope, separate from the production `COALESCE` join) | yes |
| `loan_balances` | 116 (of 116 loans; 88 carry a non-zero `total_outstanding_derived`) | 116 (`EXISTS` scope) | yes |
| `loan_arrears` | 59 | 59 (`EXISTS` scope) | yes |

Aggregate cross-check (production query output vs a plain `sum()`/`count()` over
the whole table, both office-unrestricted since every row resolves into scope
locally): `loan_charges` sums `amount=30249.95, amount_paid_derived=18750.74,
amount_outstanding_derived=11499.21` against the table's own totals — equal.
`loan_balances` sums `total_outstanding_derived=537817.37,
principal_outstanding_derived=325461.41` against `m_loan`'s own totals —
equal. `loan_arrears` carries `overdue_since_date_derived` from
`2025-11-18` to `2026-05-18` (`total_overdue_derived` sums to `71250.10`) —
every row keeps this per-row as-of date, so the XR-ASOF signal travels with
the response rather than being asserted as always-current; freshness still
depends on COB (D14), which remains `gap`, so no job-run join was added.
`loan_balances` reads `m_loan`'s own `*_derived` columns directly with no
join to `m_loan_repayment_schedule`/`m_loan_transaction` (LOAN-8 rule).
The locked `answers` stage additionally passed the three new FIN-108 slice 3
chains (loan_charges, loan_balances, loan_arrears) — see
`fineract-assistant-api/answers/loans/`.

## 14. FIN-108 slice 4 / LOAN-4, 10..12 — disbursement, collateral/guarantor, reschedule/terms-change/write-off/recovery, D15 balance tables

### 14.1 Production vs independent SQL (Fineract, all offices `{1,2,3,4,5,6,8,9}`, limit 100)

Production = `queries/loan/<id>.sql` with `$1`/`$2` bound; oracle =
`tests/answers/<id>.sql` (separately written: `UNION` scope instead of
`COALESCE`, scalar subqueries/`NOT EXISTS` instead of joins, pre-aggregated
mapping + `LEFT JOIN` for terms change). Equal = identical JSON row arrays.

| Capability | Production rows | Oracle rows | Equal |
| --- | --- | --- | --- |
| `loan_disbursement` | 100 (limit; 116 loans in source) | 100 | yes |
| `loan_disbursement_tranches` | 0 | 0 | yes |
| `loan_collateral` | 12 | 12 | yes |
| `loan_guarantor` | 1 | 1 | yes |
| `loan_guarantor_funding` | 0 | 0 | yes |
| `loan_reschedule` | 6 | 6 | yes |
| `loan_terms_change` | 6 | 6 | yes |
| `loan_writeoff` | 0 | 0 | yes |
| `loan_recovery` | 0 | 0 | yes |
| `loan_capitalized_income_balance` | 0 | 0 | yes |
| `loan_buy_down_fee_balance` | 0 | 0 | yes |

Production values: 74 of 116 loans have `net_disbursal_amount` ≠
`approved_principal` (e.g. loan 1: 5000.00 approved, 4950.00 net).
Collateral: all 12 pledges use master rows in USD (`m_collateral_management.currency`
→ `m_currency.id` 148) on AED loans — the two currencies differ, so
`collateral_currency_code` and `loan_currency_code` are both returned; loan 13:
5 × 2000.00 × 50% = 5000.00 USD, total 163000.00 USD over 12 pledges, at the
CURRENT master rate (Fineract keeps no historic pledged valuation). Guarantor:
1 row, `type_enum = 3` external, names `pii`. Reschedule: 6 requests (4
pending, 2 approved). Terms change: 6 variations (5 `due_date`, 1
`extend_repayment_period`), every one mapped exactly once — so the unmapped and
duplicate-mapping cases have no production example. Tranche, guarantor
funding/transaction, write-off, recovery and both D15 tables have **0 rows**
in this deployment: there is no production positive evidence for them.

The first slice-4 attempt read LOAN-12 from `m_loan` flags
(`loan_capitalized_balances`, 116 rows of `false`/0) and presented that and
the zero-row tables as proof. That capability is removed; D15 is now read from
the two tables `dataset-scope-decisions.md:188` names.

### 14.2 Positive behaviour — read-only fixture (a test, not evidence)

Fineract is never written. A throwaway scratch database
`fin108_fixture_scratch` (same host, separate database) received a schema-only
copy (`pg_dump -s`) of the 15 tables the queries read, with FK/CHECK/NOT NULL
relaxed so minimal rows could be inserted. The **unmodified** production SQL
files ran via `PREPARE q AS <file>; EXECUTE q('{1}', 100)` and the ordered
output was diffed against hand-written expectations. Scope: office 1 in
scope, office 2 out; L1 (client office 1, USD), L2 (client office 2, USD),
L3 (group-only office 1, EUR).

| Capability | Fixture cases | Expected = actual |
| --- | --- | --- |
| `loan_disbursement_tranches` | ids 11/12 same date (tie-break by id), 13 reversed (kept, `is_reversed=t`), 14 out of scope, 15 group-office path EUR | 11, 12, 13, 15 — yes |
| `loan_guarantor_funding` | f1 guarantor association (type 2) to GBP savings, 3 transactions (1 reversed); f2 association type 1 (not guarantor); f3 out of scope | f1: savings 1, GBP, count 2, one row; f2: savings NULL, currency NULL, count 0 — yes |
| `loan_writeoff` | L1 written off (reason "Bad debt", 400/30/15/5, total 450), L2 out of scope | one row L1 USD — yes |
| `loan_recovery` | 101 type 8 (120.00), 102 type 8 reversed, 103 office 2, 104 type 2 | only 101, USD — yes |
| `loan_terms_change` | 21 mapped once, 22 unmapped interest pause, 23 mapped twice (502, 503), 24 out of scope | 22 (request NULL, count 0), 21 (501, 1), 23 (502, 2), one row each — yes |
| `loan_capitalized_income_balance` | b1 open, b2 `is_deleted` (tx not reversed), b3 closed, b4 tx reversed (not deleted), b5 out of scope, b6 group path EUR charged off 250 | b1, b3 (`is_closed=t`), b6 — yes |
| `loan_buy_down_fee_balance` | 1 open (adj 25, unrecognized 125), 2 `is_deleted`, 3 closed EUR, 4 tx reversed | 1, 3 — yes |

Mutation check (same fixture, production SQL weakened in a copy): old
`INNER JOIN` mapping (drops 22, duplicates 23), no `association_type_enum = 2`
guard (f2 gains USD), no `is_deleted` filter, no `is_reversed` filter (cap
income and buy-down separately), no recovery reversal filter, no tranche id
tie-break — **6/6 detected**. Semantics of `is_deleted`/`is_closed` from Apache
Fineract source: `LoanAdjustmentServiceImpl` sets `deleted` when the
capitalized-income/buy-down-fee transaction is reversed;
`Loan{CapitalizedIncome,BuyDownFee}AmortizationProcessingServiceImpl` set
`closed` when amortization finishes (and `charged_off_amount` at charge-off).
Funding association per `GuarantorWritePlatformServiceJpaRepositoryIImpl`
(`associateSavingsAccount(loan, savings, GUARANTOR_ACCOUNT_ASSOCIATION=2)` →
`linked_savings_account_id`). The scratch database and scripts were removed
after the run; no permanent test was kept (the project allows no DB-backed
Rust test, and the HTTP chains below cover the production path).

### 14.3 HTTP

Locked runs on port 3308, worktree database: `answers/loans` with
`ANSWERS_ORACLE_PREFIX=loan` (21 loan oracles) — every slice-4 capability has
an English and an Indonesian chain (`*-id-*`, the capability's own Indonesian
example) asserting the same capability, parameters, columns and rows as the
oracle. The OVR-6.6 full-chain control moved from `policy-deferred-loan-*`
(LOAN-4 is answered now) to `policy-deferred-tax-full-chain-*` (still-gap
`tax`), run alone with its prerequisites (`engine/login.yml`,
`engine/policy-session.yml`). Final run: engine selection 8/8 requests, 7/7
tests; `answers/loans` 131/131 requests, 184/184 tests; OVR-6.7 zero
violations.

## 15. FIN-159 / LOAN-6, LOAN-7 — source loan currency added

Both capabilities now output the owning loan's `currency_code` (FIN-107
decision 1). Two numbers side by side, two writings: production SQL in
`queries/loan/`, independent oracle in `tests/answers/`, compared row for row
over HTTP by `answers/loans/<cap>-answer.yml` (see `docs/build-order.md`,
"Update after FIN-159", for commands and job IDs). HTTP planning in that run
used a deterministic test-fixture provider, not the real LLM.

| Capability | HTTP page (job) | Oracle page | Source population |
| --- | --- | --- | --- |
| `loan_transactions` (last 12 months, limit 100) | AED 71 / USD 29 (`e91c6b86…`) | AED 71 / USD 29 | AED 1,704 / USD 1,121 / EUR 32 / AUD 2 (2,859) |
| `loan_charges` (limit 100) | AED 46 / EUR 6 / USD 48 (`fdeece29…`) | AED 46 / EUR 6 / USD 48 | AED 155 / USD 48 / EUR 14 (217) |

Limitation: AUD/EUR transactions are outside the default first page and were
checked by SQL only; no AUD loan charge exists in this deployment.

## 16. FIN-124 / D14 — scheduler job definitions, failed runs, loan_arrears freshness

Two numbers side by side, two writings: production SQL in `queries/scheduler/`
and `queries/loan/loan_cob_coverage.sql`, independent oracle in `tests/answers/`
(`scheduler_job_failed_runs.sql`, `scheduler_job_definitions.sql`,
`loan_arrears__freshness.sql`), compared row for row over HTTP by
`answers/scheduler/*` and `answers/loans/loan_arrears-*.yml`. Database
`fineract_default`, read-only (SELECT/PREPARE only, no fixture writes). HTTP
planning used the **real** configured provider (`qwen-cloud`, `qwen3.8-flash`)
through the Rig planner; its URL needed the `/chat/completions` suffix as a
process-only override, `.env` untouched.

| Capability / probe | Window or scope | Production (HTTP job) | Independent SQL |
| --- | --- | --- | --- |
| `scheduler_job_run_history` (job 12, success AND failed, via resolver) | `April 2026 to May 2026 on the stored clock` -> `[2026-04-01 00:00:00, 2026-06-01 00:00:00)` | 33 rows (`e33921c9…`, final hash `3671aaa3…`; the earlier `024a59e0…` request did not state the stored clock): 9 failed 2026-04-01..04-09 then 24 success 2026-04-10..05-03, oldest first, latest overall `success` on the last row | 33 rows identical row for row (ids, order, text timestamps, `is_job_latest_run`, latest status) |
| `scheduler_job_failed_runs` (earlier run, knowledge hash `2d9ee04f…`, not re-run at the final hash) | `April 2026 to May 2026` -> `[2026-04-01 00:00:00, 2026-06-01 00:00:00)` | 90 rows (`bae2e8d6…`): jobs 7 = 33, 8 = 33, 42 = 15, 12 = 9; latest overall failed for 7/8/42, success for 12; 89 cron + 1 application | 90 rows, identical row for row (ids, order, text timestamps, latest status) |
| `scheduler_job_definitions` | `December 2025 to December 2025 on the stored clock` -> `[2025-12-01 00:00:00, 2026-01-01 00:00:00)` | 44 jobs (`c5aaa101…`, final hash; an earlier attempt `d2da2a83…` ended `planner_output_invalid` — model output, nothing executed — and was re-submitted unchanged); job 36 (`Send Asynchronous Events`, every minute) 44,640 runs; job 42 (active daily) 0 runs, `no_observed_run`, latest `failed` | 44 jobs identical; job 36 = 31 x 1,440 = 44,640 (runs at exactly `2025-12-01 00:00:00` counted, at `2026-01-01 00:00:00` excluded) |
| freshness batch probe `Loan COB` | read with `m_business_date` | `cob_date` 2026-09-07, `business_date` 2026-09-08, latest run `success` 2026-05-03T18:30:00.000 (run 307177) | identical |
| `loan.loan_cob_coverage` (all 8 offices) | active/transfer loans | 87 in scope, 87 behind COB, oldest last-closed 2026-02-10 | 87 / 87 / 2026-02-10 |
| calendar word on the stored clock (refusal) | `semalam (last night) on the stored clock` | rev 3 (older rule): guard `stored_clock_window_required` (`7ab9569e…`); rev 4: real model `planner_no_fit` (`ccd60f2a…`); the guard's positive-evidence rule is unit-tested | — |
| original question D, Indonesian: latest successful interest posting proves August + every account current | `2026-08-01..2026-08-31` stored clock | `limitation` / `Unknown` / `planner_no_fit` (`963d15e5…`), explicit text that run observations + an unsupported coverage claim are not answered; no rows | — |
| `loan_arrears` completeness (earlier hash, not re-run: no change to the capability, its SQL or `freshness.rs`) | same scope | `Partial` / `batch_coverage_incomplete` on node ledger, dataset handle and response (`8d6465fb…`; first run `cb1cdd30…`), 59 arrears rows = oracle 59 | — |

Why the `loan_arrears` answer is `Partial` on this fixture while a **successful**
Loan COB run exists: the newest run (2026-05-03) succeeded, yet every in-scope
loan's `last_closed_business_date` is older than the COB date, so the run proves
nothing about coverage. Counts are fixture observations only.

Time semantics (verified against the Fineract source and the data, not the DB
session or the machine): `job_run_history` timestamps are `timestamp without time
zone`; `SchedulerJobListener.jobWasExecuted` writes `context.getFireTime()` /
`new Date()` and `status` `success`|`failed`, `trigger_type` `cron` by default or
`application` (`SchedulerServiceConstants`, `JobRegisterServiceImpl.executeJob`).
Every cron-fired run in this deployment sits exactly 5 h 30 min earlier than its
cron expression (e.g. `0 0 0 ...` stored at `18:30:00`, `0 0 22 ...` at `16:30:00`,
all months), consistent with the tenant-timezone cron trigger
(`fineract.tenant.timezone` default `Asia/Kolkata`) and a UTC-storing JVM
(`-Duser.timezone=UTC`) — but `fineract_tenants.tenants.timezone_id` says
`Asia/Beirut`, so the storage zone is **not** declared by the data. The capability
therefore declares the clock as `timezone_naive_as_stored`, applies windows to it
without conversion, and invents no calendar meaning. Owner decision (2026-10-15):
**explicit stored-clock windows only** — `param_parse::stored_clock_window_issue`,
called by the planner for every `operational_tenant_wide` query with a `from_date`,
refuses (`stored_clock_window_required`) unless the request states the stored clock
AND an explicit month/ISO date range (positive evidence, no word list) with no
relative period beside it; nothing is converted or guessed.
Zero reversed (`end < start`) and zero cross-date runs were observed.

Fixture limits: no `Post Interest For Savings` (id 6), `Transfer Interest To
Savings` (14), `Recalculate Interest For Loans` (17) or `Generate Loan Loss
Provisioning` (19) run ever failed (coordinator evidence), so the mixed
failed/success mechanism is shown on jobs 7, 8, 12, 42; no two failed runs share a
`start_time`, so the `id DESC` tie-break is declared in SQL but not exercised by
data; the `Unknown` verdicts are covered by a pure test of `freshness::assess`,
not by this fixture. Freshness rule hardening (review): `Complete` needs the batch observed AND its latest run `success` AND zero loans behind COB; an absent run or unfamiliar status is `Unknown`, never guessed success (pure test).

Other batch-sensitive analyses — investigated in Fineract source and in every
`queries/*/*.sql`, not assumed:

- Savings interest posting: `m_savings_account.interest_posted_till_date` is set to
  the transaction date of the latest non-reversed interest-posting (or overdraft-
  interest) transaction (`SavingsAccountSummary.updateRunningBalanceAndPivotDate`).
  `PostInterestForSavingTasklet` / `retrieveAllSavingsDataForInterestPosting`
  selects, on every run, ACTIVE accounts whose value is NULL or <= business date - 1.
  So the column is a recorded fact about the last posting, not a coverage marker:
  no transaction exists when interest is zero or no posting period ended, and being
  "behind" is the normal state between periods. Fixture (171 active accounts): 85 monthly accounts at
  `2026-09-01`, 43 daily at `2026-09-09`/`2026-09-10` (after the business date
  2026-09-08), 25 never posted, 18 older. No account-level fact proves "posted through the business date",
  so coverage is **not derivable** — and no implemented query reads a posting-
  dependent column (`interest_posted_till_date`, `total_interest_*`,
  `last_interest_calculation_date`), so no capability receives a freshness claim.
  `savings_balance_summary` sums recorded `account_balance_derived`: it is not lowered.
- Provisioning (D07): `m_provisioning_history` has 0 rows and no capability exists.
- Columns written by Loan COB / arrears jobs (`m_loan_arrears_aging`, `last_closed_business_date`)
  are read only by `loan_arrears` (and its coverage probe) — the one hand-off.
- RD overdue (`total_overdue_amount`, `no_of_overdue_installments`, read by
  `savings_deposit_maturity_by_client`) is written by `updateOverduePayments` on
  activation/transaction/undo/adjust, not by a scheduler job: a recorded value, not a batch result.

Remaining honest gaps: a question about interest-posting *freshness* is answered
only with run observations (scheduler responses state that a run proves no business
result) and stays a declared `gap_intent`; the `job_run_history` storage timezone is
undeclared, so only stored-clock windows are accepted.

Real-model note (2026-10-15): the scoped scheduler chains (definitions 44 = SQL 44,
run history and failed runs row-for-row equal to independent SQL, refusals) passed on a
second scoped run; the first run failed at the run-history form (provider timeout, then
`static_verification_rejected`) and is retained as failed evidence. A single-month
date-slot quote ("April 2026") was a demonstrated verifier gap and is now bound; the
run-history `no_fit` and the uncaptured rejection are provider-reliability limits, not a
catalog defect. This is not a claim that planning is reliable.


## 17. FIN-115 / D06, D15e — teller/cashier assignments, recorded cash and inter-office transfers

**Correction to the inventory (recorded here; the contract rows are not edited).** `dataset-inventory.md`
D06 says no cash-movement table exists. The live schema disproves it: `m_cashier_transactions`
(49 rows: 38 × type 101 allocate, 11 × type 102 settle, no 103/104; `entity_type` empty and
`entity_id` NULL on every row; `currency_code` never NULL here; `txn_date` 2025-11-10..2026-04-18),
6 `m_cashiers` (teller FK, staff FK, own start/end date, `full_day`), 6 `m_tellers`
(office 2,3,4,5,6,8; `state` 300) and 0 `m_office_transaction` rows. No reconciliation or counted-cash
table exists, so physical cash and cash discrepancy stay `gap_intents`.

**Upstream semantics** (apache/fineract `develop`, `CashierTxnType`, `CashierTransaction`,
`TellerManagementReadPlatformServiceImpl`): 101 Allocate, 102 Settle, 103 Cash In, 104 Cash Out. The
reader's summary UNIONs the stored journal with SAVINGS, LOAN and CLIENT transactions attributed to a
cashier through `created_by -> m_appuser.staff_id -> m_cashiers.staff_id` inside the cashier's
`[start_date, end_date]`, excluding reversed and account-transfer rows and accepting
`payment_detail_id IS NULL OR is_cash_payment`; it also labels waivers, write-offs and interest as cash
out. Allocation minus settlement is therefore NOT the complete cashier position. Local evidence: loans
0 attributed rows; savings 14 attributed rows, 6 are internal transfers (excluded) and 8 have an
explicit cash payment type (USD in 3370, USD out 130, AED out 20 — cashier 2 USD in 800; cashier 3 USD
in 800 / out 130; cashier 4 USD in 1770, AED out 20); client transactions 0.

**Built (all Mode 1, explicit window, bound `office_ids`, limit hard cap 200 with disclosed row cap):**
`teller_cashier_assignments` (grain cashier), `cashier_cash_movements` (grain journal row),
`cashier_recorded_cash_summary` (grain cashier + currency; journal only; every row carries
`source_coverage`), `cashier_savings_cash_summary` (grain cashier + currency; the savings branch only,
explicit cash payment type, inferred attribution stated on every row, never added to the journal
totals) and `inter_office_cash_transfers` (D15e, separate; visible only when every recorded office is
authorized). Not built, kept as `gap_intents`: physical cash count, cash reconciliation, cash
discrepancy/shortage/overage, the complete cash position and loan/client derived cash. Staff is the
surrogate `staff_id`; staff/teller names, descriptions and `txn_note` are never selected.

### 17.1 Production vs independent SQL (HTTP, real model, all authorized offices `{1,2,3,4,5,6,8,9}`)

Independent oracles are `tests/answers/{teller_cashier_assignments,cashier_cash_movements,
cashier_recorded_cash_summary,cashier_savings_cash_summary,inter_office_cash_transfers}.sql` (+ `__scope_2_3`
variants). Locked run 3 (`answers/teller_cashier`): 127 requests / 63 tests, columns, bound parameters,
scope count and every row equal.

| Capability | Window | Rows (all offices) | Rows (`office_ids` [2,3]) | Job ids (all / narrowed) |
| --- | --- | --- | --- | --- |
| teller_cashier_assignments | February 2026 | 5 (cashier 4 ended 2025-12-31, absent) | 2 | 8a185302… / 9c7745b1… |
| cashier_cash_movements | Jan–Mar 2026 | 31 | 2 | 5d92aa01… / 93875cc8… |
| cashier_recorded_cash_summary | Nov 2025–Apr 2026 | 16 | 4 | 357c7d4b… / 38516773… |
| cashier_savings_cash_summary | Nov–Dec 2025 | 4 (USD in 3370, out 130; AED out 20) | 1 | 7bbe1307… / 992f5371… |
| inter_office_cash_transfers | Jan–Jun 2026 | 0 (`Empty`) | n/a | 8d25ea9c… |

### 17.2 Failed evidence (retained, not rerun blindly)

- Run 1 (`/tmp/fin115-evidence/run1-*`): summary job `08f5a855…` `planner_output_invalid`, 0 model calls.
- Run 2 (`run2-*`): assignments job `90b37fde…` same failure. Cause (read from
  `crates/chat/src/engine/llm.rs`): before any model call the planner refuses when
  `SYSTEM + input + request + 128 > LLM_MAX_INPUT_TOKENS` (default 16000, byte-counted). With the
  shortlist of 6 the five new, similarly worded capabilities (~2.1-2.3 KB of candidate JSON each) pushed
  the prompt over. No planner or limit change was made; the descriptions, supported intents and four
  redundant output columns were shortened, and test requests keep the shortlist inside the budget.
  Open risk: any unusually worded cashier request whose shortlist is filled by long candidates can still
  hit this pre-model refusal; that is a catalog-size/planner-budget property, not a data error.
- Process-only override used: `PLANNER_PROVIDER_URL=.../compatible-mode/v1/chat/completions` because the
  checkout `.env` `LLM_CHAT_COMPLETIONS_URL` lacks the `/chat/completions` suffix and fails config
  validation; `.env` was not edited.

### 17.3 CARRY-OVER rules

1. Examples run end to end through the real model (above). 2. Prose matches SQL (reviewed per query
manifest `checks`). 3. All output fields are `public_business` ids, dates, amounts, currency codes and
constant disclosure strings. 4. Semantics: closed `txn_date` window, no as-of snapshot, no balance claim.

Unproven: nonzero inter-office transfers (empty fixture, no fixtures added), 103/104 journal rows, loan
and client derived cash, an assignment with `full_day=false`, a staff with overlapping assignments, a
savings transaction created at an out-of-scope office by an in-scope cashier.

### 17.4 Completion correction — recorded cash POSITION (additive; supersedes the narrower window summaries)

**Formula (local Fineract source, `TellerManagementReadPlatformServiceImpl`, independently confirmed by the
coordinator):** position = allocate (101) + cash in − cash out − settle (102) per cashier and currency,
where cash in/out are the stored journal rows (103/104) plus the cash derived from savings, loan and client
transactions attributed through `created_by -> m_appuser.staff_id -> m_cashiers.staff_id` inside the
cashier's assignment dates (savings `end_date`; loan/client `end_date + 1 day`, as the reader does).

**Built:** `cashier_recorded_cash_position` (grain cashier + currency). `to_date` is the cutoff and the
position is cumulative from the beginning; `from_date` only splits out `position_before_window_amount`
(the planner contract requires both slots for an explicit period, so both carry meaning). All component
columns are returned (journal, savings, loan, client in/out), so no branch is hidden. A group exists when
any flow row exists and the position is the signed SUM, so a group that nets to zero is returned as 0.
Not physical, counted or reconciled cash; no reconciliation record is read.

**Superseded and removed:** `cashier_recorded_cash_summary` (journal-only window totals) and
`cashier_savings_cash_summary` (savings-only window totals), with their oracles and Bruno cases. Their
figures are now columns of the position; their §17.1 proofs stay as history of the earlier slice. Kept:
`teller_cashier_assignments`, `cashier_cash_movements` (row-level journal movements),
`inter_office_cash_transfers`.

**Declared divergences from the Fineract reader (disclosed in `position_basis`):** only
`is_cash_payment = true` counts (the reader also counts `payment_detail_id IS NULL`); waivers, write-offs and
interest are not cash (the reader labels them cash out); attribution is inferred, not a stored link; the
journal is cumulative by `txn_date` (the reader filters `created_date` against the assignment dates, except
allocations).

**Proof (cutoff 2026-04-30, window start 2026-04-01; independent SQL `tests/answers/cashier_recorded_cash_position.sql`
and `__scope_2_3`):** 16 groups for all authorized offices `{1,2,3,4,5,6,8,9}`, 4 groups for `office_ids` [2,3].
Required values hold: cashier 3 / USD = 0 (journal 26000 − 26670 = −670, plus savings cash in 800 and out 130),
cashier 2 / USD = 20800, cashier 4 / USD = 35970 and AED = 25000; cashier 6 shows a real window movement
(USD 0 before April, 10000 at the cutoff). Real-model jobs: `994d8df8-e939-4540-93cd-f79ea0f64a29` (all offices) and
`5e8851f1-1ed8-4025-9d64-1cdb94b2bb53` (narrowed); locked run `answers/teller_cashier_position` +
`answers/teller_cashier_diagnostic`: 33 requests / 14 tests passed (`/tmp/fin115-evidence/position-run5-pass.out`).
The eight explicit savings cash events (cashier 2 USD in 800; cashier 3 USD in 800, out 20 and 110; cashier 4
USD in 800, 100 and 870, AED out 20) are inside these totals.

**Failed attempts kept (not rerun blindly), `/tmp/fin115-evidence/position-run{1..4}-*`:**
- `b74bcd50…` pre-model `planner_output_invalid`, 0 model calls: with six teller capabilities the candidate
  prompt exceeded the 16000-byte input budget (the position candidate is ~3.0 KB). Resolved by removing the two
  superseded summaries, not by dropping fields or changing limits.
- `aee9b29f…`, `2b50e077…` (model called, `static_verification_rejected`): the first shape exposed only `to_date`,
  but the planner contract requires both date slots for an explicit period; then the wording "through April 2026"
  was rejected after four attempts (three provider timeouts at 30 s, then an invalid proposal). The accepted chain
  asks "for April 2026". Cause of the second rejection is inferred (model quote), not captured.
- `98cd75a2…`: four `planner_unavailable` provider timeouts (30 s each), no usage recorded.

**Retained diagnostic (not acceptance):** the original English assignments request "Show the teller and cashier
assignments that overlap February 2026." (job `13fbf2fe-4a9a-42a2-9729-2367ab4fca8c`) now reaches the model
(3584 input tokens, so the budget is no longer the cause) but its proposal is `static_verification_rejected`
(`planner_output_invalid`). It is recorded honestly; wording was not used to turn it green.

**Remaining limits:** loan and client branches follow the Fineract rules but have 0 attributed rows locally, so
they are unproven by data; journal types 103/104 have no local rows; a staff member with overlapping
assignments would be counted for each (fan-out) and this is unproven; `full_day = false` assignments;
nonzero inter-office transfers; counted physical cash and reconciliation remain gap intents.


### 17.5 Final integration correction — anti-fanout attribution (current revision; additive)

**Defect found in review:** all three derived branches of `cashier_recorded_cash_position.sql` joined `m_cashiers` on the creator's
staff and the date bounds, so a staff member holding overlapping assignments would put the same financial event into several
cashiers' positions. The earlier statement that overlap was merely "unproven" was wrong: the contract forbids double counting, so
this was a defect, not a missing fixture. The oracle `tests/answers/cashier_recorded_cash_position.sql` shared the same join shape.

**Fix (current revision):** every derived event (savings, loan, client) is classified BEFORE summing by the number of cashier
assignments that match it, counted over ALL assignments (ambiguity is a property of the event). Exactly one match = admitted.
More than one = ambiguous: added to no cashier's amount, counted in `ambiguous_event_count` of each in-scope candidate group, and
the group's `recorded_cash_position_amount` (and `position_before_window_amount` when an ambiguous event predates the window)
is NULL with `position_status = 'withheld_ambiguous_attribution'`; otherwise `'complete'`. Scope: the event's own office must be
authorized and only candidate cashiers whose teller office is authorized are returned, so no out-of-scope cashier, amount or count
is exposed (an out-of-scope candidate still makes the event ambiguous, which is the safe direction). The oracle was rewritten
independently (correlated scalar subquery for the match count) and still yields the original values: cashier 3 / USD = 0,
cashier 2 / USD = 20800, cashier 4 / USD = 35970 and AED = 25000, all `complete` with 0 ambiguous events locally.

**Deterministic smoke (no Fineract table read, no fixture):** `tests/position_attribution_smoke.sh` runs the production SQL text
unchanged with every Fineract table shadowed by an inline `VALUES` CTE. Verified: two overlapping assignments of one staff
→ event ambiguous, both groups withheld with count 2 (not a fake 0); a June event that also matches a cashier in an unauthorized
office stays ambiguous and nothing of that cashier is returned; two non-overlapping adjacent assignments of another staff →
each event admitted to exactly one cashier; a cashier netting exactly 0 is returned as 0; a NULL-currency journal row is its
own group; no duplicate group. 6 checks ok.

**Assignments prose/output agreement:** `teller_cashier_assignments` again returns `teller_state_enum` next to `teller_state`
(oracles and api-reference updated), matching the description "teller validity and state". Other columns trimmed earlier for
the planner budget on `cashier_cash_movements` (teller_id, staff_id, txn_type_enum) were NOT restored in this correction.

**Runtime proof of the current revision (real model, locked run, targeted):** `answers/teller_cashier_position` (+ the changed
assignments path + diagnostic) 58 requests / 29 tests passed (`/tmp/fin115-evidence/final-run1-pass.out`). Position jobs
`e9d6b12e-c518-46d3-92ac-313f17b3356e` (all offices, 16 rows) and `7933fcb2-8fdc-45dd-b8c7-ea995ff0ccf7` (`office_ids` [2,3], 4 rows);
assignments `edc68cc7-99af-45f3-8f5d-220423376e0d` (5 rows) and `dd5a6662-a0ae-4ea6-b313-0443f24e0314` (2 rows). The
old-revision jobs `994d8df8…`/`5e8851f1…` belong to the previous revision (without ambiguity columns) and are history.
Diagnostic job `1b9efcca-4b4a-4f18-aeed-11464ce033fc`: the original assignments request now fails BEFORE any model call
(`planner_output_invalid`, 0 model calls) because the larger position candidate (two more columns) pushes the six-candidate prompt over
the 16000-byte planner input budget (offline estimate ~16.6 KB). Recorded, not worked around.

**Exact remaining limits:** loan and client branches have no nonempty acceptance proof (0 attributed locally); the VALUES smoke
executes their empty branches and proves savings attribution ambiguity, not nonzero loan/client semantics. Journal types 103/104
have no local rows; `full_day = false` assignments; nonzero inter-office rows;
NULL payment details are not counted; counted physical cash and reconciliation stay unsupported. Graph after this correction:
`graphify-out/graph.json` 2754 nodes / 5712 edges / 365 communities.

The diagnostic chain is not a permanent acceptance test: its request files were archived at
`/tmp/fin115-evidence/diagnostic-requests-20261016.tgz` and removed from the collection after observation.
The final run's 29 tests include one diagnostic observation, so only 28 are positive acceptance assertions.
The coordinator independently ran the six VALUES-only attribution checks against the application database;
the smoke defaults to `jarvis` and reads no Fineract table.

### 17.6 Lossless planner-candidate encoding and the original assignments request (completion revision; additive)

**Defect (code, not data):** `docs/architecture/engine.md` "Input and retrieval" requires each candidate to carry its complete
typed contract and the input to be bounded. `crates/chat/src/engine/llm.rs` serialized every output column as a three-member object and
repeated `"default":null,"hard_cap":null,"resolver":null` per parameter. Six candidates for
"Show the teller and cashier assignments that overlap February 2026." measured ~16.6 KB (observed, jobs `13fbf2fe…`, `1b9efcca…`), so the
job was refused before any model call (`planner_output_invalid`, 0 model calls) by the conservative byte bound. Earlier text in §17 that
calls this a catalog-size property or works around it by wording is superseded by this section.

**Fix (encoding only):** output is `{"columns":["name","type","sensitivity"],"rows":[[…],…]}` and a parameter omits `default`,
`hard_cap` or `resolver` when none is declared. Every column, parameter, intent and guard stays. The bound, shortlist, candidate selection,
token reservation, usage accounting, limits, retries, timeout, prompt and verifier are unchanged; the conservative rejection when the full
input genuinely does not fit is kept (`exceeds_input_bound`).
Regression (`original_assignments_request_context_fits_only_with_the_lossless_encoding`): the six contracts retrieved for the original
request (assignments, position, movements, scheduler_job_definitions, loan_product_bucket_assignments, collateral_pledges, real eight-office
scope) exceed 16,000 bytes in the old representation and fit in the new; `input_genuinely_over_the_bound_is_still_rejected` keeps the refusal.

**Runtime proof (real configured qwen-cloud/qwen3.8-flash, locked runner port 3107, main `jarvis` database, login + sessions), final compact-only code:**
targeted run of the original assignments chain (all offices and `office_ids` [2,3]) plus `answers/teller_cashier_position`: 45 requests / 28 tests PASS (`/tmp/fin115-evidence/final-compact.out`). Assignments jobs `5fda7d49-9c81-4bbd-9e40-598c104be085` (5 rows) and `04ca8a19-35d8-40de-9d4f-4f342ce0c06a` (2 rows): one model call each, observed input 3153 / 3141 tokens; position jobs `6c2eabff-a0df-403f-9c3d-66c6a0f6a3d1` (16 groups) and `ae0b5f05-467f-475e-873c-19618a36ce39` (4 groups): one call each, 2973 / 2961 tokens. Columns, bound parameters, scope and rows equal the independent SQL. Movements, inter-office, savings and scheduler were not re-run: they passed earlier under this same encoding (`completion-run1/2/3.out`; scheduler `65526cc4…` hit 3 provider timeouts then succeeded, unresolved separate class).

**VALUES-only semantics smoke (synthetic proof, NOT source acceptance):** `tests/teller_cash_semantics_smoke.sh` executes the unchanged
production SQL with every Fineract table shadowed by inline `VALUES` (no Fineract table read, no fixture): loan repayment-type in /
disbursement out, loan `end_date + 1` boundary, client PAY_CHARGE in, journal 103/104, unrecognized journal type (counted, not summed),
exclusion of non-cash, NULL payment detail, reversed, account-transfer, waiver and out-of-scope-office rows (position 1925 USD,
before-window 1190, 0 ambiguous); a `full_day = false` assignment and two overlapping assignments of one staff member each returned once;
nonzero inter-office rows with out-of-scope and office-less transfers hidden. 4 checks ok.

**Exact acceptance scope:** Mode-1 read semantics for assignments, stored movements, recorded position (with ambiguity withholding) and
inter-office transfers are proven by (a) real HTTP answers equal to independent SQL on the local data and (b) deterministic unchanged-SQL
VALUES smokes for branches the local data leaves empty. No nonzero real data exists locally for 103/104 rows, attributed loan/client cash,
`full_day = false` assignments or inter-office transfers (the real HTTP answer for inter-office is an actual Empty = SQL 0); no fixture was
created and no real-data nonzero claim is made. Counted physical cash and reconciliation stay unsupported gap intents. Scheduler
provider-timeout behaviour (separate FIN-124/161 class) is unresolved and unrelated to this fix.

**Inventory (owner-authorized docs commit, separate from the code):** `docs/data/dataset-inventory.md` now records the observed source mapping
(D06 named the wrong table; `m_cashier_transactions` exists) and moves D06 and D15e from `gap` to `inherited` for the approved,
source-supported Mode-1 capabilities ONLY. This is not full-domain completeness: counted physical cash, reconciliation and
discrepancy/shortage/overage remain explicit gap intents, and no L2 promotion or numbered scenario is claimed. The earlier statements in
this section that the rows stay `gap` were written before that owner decision and are history.

## 18. FIN-113 / D04 — direct client charges: assessments and payment allocations

**Context & Scope.** `dataset-inventory.md` row D04 covers direct client charges (`m_client_charge` and `m_client_charge_paid_by`). Local `fineract_default` database has 0 rows in both tables. As required by FIN-113 and mirroring FIN-115 / D15e, capabilities are delivered as Mode-1 curated queries with independent SQL oracles for the Empty population, plus an inline VALUES-only CTE test script (`tests/client_charge_semantics_smoke.sh`) proving positive non-zero semantics on unmodified production SQL.

**Built (Mode 1, explicit window, bound `$1::bigint[]` authorized office scope, limit hard cap 200 with disclosed row cap):**
1. `client_charge_assessments` (grain client charge assessment, `[client_charge_id]`, sorting `due_date DESC, client_charge_id DESC`):
   - Assesses operational charges (`m_client_charge`) joined to charge masters (`m_charge`) and clients (`m_client`).
   - Office scope: strictly authorizes on client office (`c.office_id = ANY($1::bigint[])`), matching data-scope boundaries.
   - Derived amounts: nullable derived columns (`amount_outstanding_derived`, `amount_paid_derived`, `amount_waived_derived`, `amount_writtenoff_derived`) are left NULL if uncomputed, never coerced to 0.00.
   - Reversals & waives: surfaced explicitly via `is_reversed` and `is_waived` boolean flags, never filtered out silently.
   - Multi-currency: reported per row with explicit `currency_code`, never cross-currency aggregated.
   - PII exclusion: client names, notes, and external identifiers are excluded; only `client_id` and charge identifiers are returned.
2. `client_charge_payment_allocations` (grain payment allocation, `[allocation_id]`, sorting `transaction_date DESC, allocation_id DESC`):
   - Allocations from client payments to assessed charges (`m_client_charge_paid_by` `ccp` joined to `m_client_charge` `cc`, `m_client_transaction` `ct`, `m_client` `c`, and `m_charge` `ch`).
   - Grain preservation: one row per allocation (`ccp.id`), avoiding Cartesian product / fan-out.
   - Office scope & audit evidence: authorized on client office (`c.office_id = ANY($1::bigint[])`); transaction office (`ccp.office_id`) is projected as audit evidence, never used for authorization.
   - Reversals: transaction reversal (`ct.is_reversed`) surfaced explicitly.
   - Currency: reported with charge currency (`ch.currency_code`), matching financial allocation grain.

### 18.1 Production vs independent SQL (HTTP, all authorized offices `{1,2,3,4,5,6,8,9}`)

Independent oracles are `tests/answers/{client_charge_assessments,client_charge_payment_allocations}.sql` (+ `__scope_2_3` variants).
Locked run (`answers/client_charges`): 21 requests / 28 tests passed (`OVR-6.7: nol pelanggaran isolasi commit`), columns, bound parameters, scope count, and row-for-row equality asserted.

| Capability | Window | Rows (all offices) | Rows (`office_ids` [2,3]) | Job ids (all / narrowed) |
| --- | --- | --- | --- | --- |
| client_charge_assessments | 2025-10-15..2026-10-15 | 0 (`Empty`) | 0 (`Empty`) | 83b48ce0… / eba9d519… |
| client_charge_payment_allocations | 2025-10-15..2026-10-15 | 0 (`Empty`) | 0 (`Empty`) | 3beeb9a2… / a1fa33f3… |

### 18.2 VALUES-only semantics smoke (`tests/client_charge_semantics_smoke.sh`)

Because production tables have 0 rows in the local database, `tests/client_charge_semantics_smoke.sh` shadows all underlying Fineract tables (`m_client_charge`, `m_charge`, `m_client`, `m_client_charge_paid_by`, `m_client_transaction`) using inline `VALUES` CTEs with unmodified production SQL text:
1. **Grain & No Fan-Out**: Multiple allocations (2 payment rows for charge 101) return distinct rows with exact allocation IDs (`201`, `202`) and individual allocation amounts (`50.00`, `25.00`), never summing or multiplying.
2. **Reversals Surfaced**: Reversed assessment (`is_reversed = true`) and waived assessment (`is_waived = true`), as well as reversed payment transaction (`is_reversed = true`), are projected faithfully with explicit boolean flags.
3. **Office Authorization vs Audit**: Client in authorized office 2 is included; client in unauthorized office 99 is filtered out, even if payment transaction was handled in office 2.
4. **Per-Currency Separation**: Charges in USD and EUR are returned as distinct rows with their own `currency_code`, never cross-currency aggregated.
5. **NULL Preservation**: Nullable derived amounts (`amount_outstanding_derived = NULL`) remain `NULL`, not converted to `0.00`.
All 5 assertion checks pass cleanly.

### 18.3 CARRY-OVER rules

1. **Examples run end to end through real job execution**: Verified via locked Bruno suite `answers/client_charges`.
2. **Prose matches SQL**: Reviewed and verified in query and capability manifests `checks` sections.
3. **Output sensitivity**: Only `public_business` identifiers, dates, amounts, currency codes, and boolean flags are returned; client personal data is excluded.
4. **Semantics**: Explicit date windows (`due_date` / `transaction_date`), client office authorization, no delinquency inference, no balance sheet claim.

### 18.4 Inventory & Domain Gap Alignment

`docs/data/dataset-inventory.md` row D04 moved to `inherited` (mirroring D15e fixture wording).
`knowledge/domains/client.yaml` updated: `client charge` moved from `gap_intents` to `supported_intents` as two capabilities (`client charge assessments` and `client charge payment allocations`).
`fineract-assistant-api/engine/policy-deferred-client-charge-job.yml` updated to assert remaining client domain gap: `client loan account roster` (CLI-5).



## 19. FIN-110 / FDRD-1..10 — fixed and recurring deposits: identity, terms, funding, contributions, and shortfall

**Context & Scope.** `dataset-inventory.md` §12 rows FDRD-1..10 cover the domain `fixed_recurring_deposit`. In the local `fineract_default` database:
- `m_savings_account`: 36 FD/RD accounts (`deposit_type_enum` 200 = Fixed Deposit: 14 accounts; 300 = Recurring Deposit: 22 accounts; 100 = Savings: 171 accounts).
- `m_deposit_account_term_and_preclosure`: 36 rows (1:1 with FD/RD accounts). All `deposit_period_frequency_enum` and `min_deposit_term_type_enum` are 2 (`MONTHS`). `maturity_amount` is a projection, not an actual payout. `on_account_closure_enum` is 100 (`WITHDRAW_DEPOSIT`), 200 (`TRANSFER_TO_SAVINGS`), 300 (`REINVEST_PRINCIPAL_AND_INTEREST`), 400 (`REINVEST_PRINCIPAL_ONLY`); in local data, 35 are NULL and 1 is 200 (surfaced faithfully as instruction, never presumed as payout).
- `m_deposit_account_recurring_detail`: 22 rows (1:1 with RD accounts). All have `is_calendar_inherited = false`.
- `m_savings_account_transaction`: initial funding deposits (`transaction_type_enum = 1`) occur at `activatedon_date` with `is_reversed = false`.
- FDRD-9 schedule table source confirmed in Fineract: `m_mandatory_savings_schedule` (727 rows, all `deposit_type_enum = 300`). Generated via `RecurringDepositAccount.java:1162` (`generateSchedule`), `RecurringDepositScheduleInstallment.java:39`, and `GenerateRdScheduleTasklet.java:58`. Confirmed as underlying source and kept as explicit gap intent (`gap_intents`) for a standalone schedule query.

**Built (Mode 1, explicit window, bound `$1::bigint[]` authorized office scope, limit hard cap 200 with disclosed row cap):**
1. `fd_rd_accounts` (FDRD-1 / FDRD-4):
   - Lists FD and RD accounts in authorized client offices (`c.office_id = ANY($1::bigint[])`).
   - Grain: `savings_account_id`.
   - Distinct from savings accounts (`deposit_type_enum IN (200, 300)`).
   - Identity & privacy: `account_no` and `external_id` are never returned.
2. `fd_rd_term_preclosure` (FDRD-2 / FDRD-6):
   - Fixed and recurring deposit terms and preclosure conditions.
   - Grain: `term_id` (`m_deposit_account_term_and_preclosure.id`).
   - Surfaces `deposit_amount`, `maturity_amount` (projection), `maturity_date`, deposit periods and frequencies, penal interest flags and rates, and `on_account_closure_enum` instruction.
3. `fd_rd_funding` (FDRD-3):
   - Recorded deposit transactions near submission/activation (`transaction_type_enum = 1`, `transaction_date >= submittedon_date`, within 30 days of `activatedon_date`).
   - Grain: `transaction_id`.
   - Reversals excluded from actual funding and surfaced via `is_reversed`.
   - Prosa: "funding" is an operational reading of recorded deposits, not a distinct Fineract event type.
4. `rd_contributions` (FDRD-8 / FDRD-9):
   - Recurring deposit contributions comparing expected configuration (`mandatory_recommended_deposit_amount`) and actual recorded deposits (`SUM(amount)`).
   - Expected vs actual kept strictly in separate columns, never blended into a performance score.
   - Anti-fanout: scalar subquery on transactions preserves account grain (`savings_account_id`).
   - Surfaces `deposit_period`, frequency, `is_calendar_inherited`, and `is_mandatory`.
5. `rd_shortfall` (FDRD-10):
   - Total overdue amount and number of overdue installments from `m_deposit_account_recurring_detail`.
   - Stored batch-computed facts, strictly distinct from loan arrears (never touches loan tables).
   - Nullable overdue amounts stay NULL if uncomputed, never coerced to 0.

### 19.1 Production vs independent SQL (HTTP, all authorized offices `{1,2,3,4,5,6,8,9}`)

Independent oracles are `tests/answers/{fd_rd_accounts,fd_rd_term_preclosure,fd_rd_funding,rd_contributions,rd_shortfall}.sql` (+ `__scope_2_3` variants).
Locked run (`answers/fd_rd`): 60 requests / 71 tests passed (`OVR-6.7: nol pelanggaran isolasi commit`), columns, bound parameters, scope count, and row-for-row equality asserted.

| Capability | Window | Rows (all offices) | Rows (`office_ids` [2,3]) | Job ids (all / narrowed) |
| --- | --- | --- | --- | --- |
| fd_rd_accounts | 2026-01-01..2026-03-31 | 23 | 5 | 8847d4e5… / 06d15f1a… |
| fd_rd_term_preclosure | 2026-04-01..2026-06-30 | 9 | 1 | 8c9b9ec2… / 3b22295d… |
| fd_rd_funding | 2026-01-01..2026-03-31 | 13 | 4 | dcc4c619… / 3fe8ba6c… |
| rd_contributions | 2026-01-01..2026-03-31 | 16 | 2 | efa2e896… / c0334e32… |
| rd_shortfall | 2026-01-01..2026-03-31 | 16 | 2 | 26fa45d6… / caa9df9a… |

In addition, office 2 empty population tested (`fd_rd_accounts`): 0 rows returned with `outcome = "Empty"`.

### 19.2 VALUES-only semantics smoke (`tests/fdrd_semantics_smoke.sh`)

`tests/fdrd_semantics_smoke.sh` shadows all underlying Fineract tables (`m_savings_account`, `m_client`, `m_deposit_account_term_and_preclosure`, `m_deposit_account_recurring_detail`, `m_savings_account_transaction`) using inline `VALUES` CTEs with unmodified production SQL text:
1. **Accounts Scope & Type Filtering**: Includes FD (200) and RD (300) in authorized office 2; filters out savings accounts (100) and accounts in unauthorized office 99.
2. **Term & Preclosure Semantics**: Includes terms within maturity date window; projects `maturity_amount` (25000.00), penal interest, and `on_account_closure_enum` (200); filters out terms outside the date window.
3. **Funding Initial Deposit & Reversals**: Includes deposit transaction at activation; filters out reversed deposit (`is_reversed = true`) and out-of-window deposits; excludes account numbers.
4. **Contributions Anti-Fanout & Granularity**: Scalar subquery aggregates multiple transactions (2 deposits totaling 3000.00) without fan-out; preserves expected (`1500.00`) vs actual (`3000.00`) in separate columns; projects `is_calendar_inherited = false`.
5. **Shortfall Batch Facts & NULL Preservation**: Surfaces `total_overdue_amount` and `no_of_overdue_installments`; uncomputed account retains `NULL` (never coerced to 0); distinct from loan arrears.
All 5 assertion checks pass cleanly.

### 19.3 CARRY-OVER rules

1. **Examples run end to end through real job execution**: Verified via locked Bruno suite `answers/fd_rd`.
2. **Prose matches SQL**: Reviewed and verified in query and capability manifests `checks` sections.
3. **Output sensitivity**: Only public business identifiers, dates, amounts, currency codes, and flags are returned; `account_no` and `external_id` are never returned.
4. **Semantics**: Explicit date windows, client office authorization, no delinquency confusion with loan arrears, maturity amount treated as projection.

### 19.4 Inventory & Domain Gap Alignment

`docs/data/dataset-inventory.md` rows FDRD-1..4, 6, 8, 10 moved to `inherited`. Shared rows FDRD-5 and FDRD-7 reference existing savings capabilities. FDRD-9 installment schedule confirmed to originate from `m_mandatory_savings_schedule` and kept as explicit `gap_intents` in `knowledge/domains/fixed_recurring_deposit.yaml`.
`fineract-assistant-api/engine/policy-deferred-fdrd-job.yml` updated to assert remaining FDRD-9 gap intent ("Show the recurring deposit installment schedule for this account.").

## 20. FIN-111 / SHARE-1..8 — share accounts, transactions, charges, payment allocations, and dividends

**Context & Scope.** `dataset-inventory.md` §11 rows SHARE-1..8 cover the domain `share`. In the local `fineract_default` database:
- `m_share_account`: 2 share accounts (`status_enum` 300 = Active: 2 accounts). `submitted_date` 2025-11-01 and 2025-12-01. Total approved shares: 500 and 1000.
- `m_share_account_transactions`: 3 transactions (`type_enum` 500 = Purchase, `status_enum` 300 = Approved). Transaction dates in 2025-11 and 2025-12.
- `m_share_account_charge`: 2 charges assessed (`charge_time_enum` 13 = Activation).
- `m_share_account_charge_paid_by`: 2 allocation rows (`pb.amount` 50.00 and 100.00), linking share charges and share transactions.
- `m_share_account_dividend_details`: 0 dividend rows in local database. Genuine Empty outcome with `completeness = Complete`.
- SHARE-7 settlement proof: not tracked in Fineract data model; retained as explicit gap intent (`gap_intents`) for domain `share`.

**Built (Mode 1, explicit window for accounts/transactions, bound `$1::bigint[]` authorized office scope, limit hard cap 200 with disclosed row cap):**
1. `share_accounts` (SHARE-1):
   - Lists share accounts in authorized client offices (`c.office_id = ANY($1::bigint[])`).
   - Grain: `share_account_id` (`m_share_account.id`).
   - Bounded by submission date window: `$2::date <= sa.submitted_date AND sa.submitted_date <= $3::date`.
   - Distinct lifecycle status enums: 100=Submitted, 200=Approved, 300=Active, 500=Rejected, 600=Closed (`ShareAccountStatusType.java:30`).
   - Surfaces `total_approved_shares` and `total_pending_shares`.
   - Identity & privacy: `client_name`, `account_no`, and `external_id` are never returned.
2. `share_account_transactions` (SHARE-2):
   - Share purchases, redemptions, and charge payments on share accounts in authorized offices.
   - Grain: `transaction_id` (`m_share_account_transactions.id`).
   - Bounded by transaction date window: `$2::date <= st.transaction_date AND st.transaction_date <= $3::date`.
   - Distinct transaction type enums: 500=Purchase, 600=Redeem, 700=Charge Payment (`PurchasedSharesStatusType.java:23`, `ShareAccountTransaction.java:49-53`).
   - Surfaces `total_shares`, `unit_price`, `amount`, and `charge_amount`.
3. `share_account_charges` (SHARE-3):
   - Fee charges assessed on share accounts in authorized client offices.
   - Grain: `charge_id` (`m_share_account_charge.id`).
   - Parameters: `office_ids` + `limit` (no date column on source table).
   - Surfaces `charge_time_type`, `charge_calculation_type`, `charge_payment_mode` (`ChargeTimeType.java:25`).
   - Preserves NULL for derived amounts (never coalesced to 0).
4. `share_charge_payment_allocations` (SHARE-5):
   - Payment allocations against share account charges (`m_share_account_charge_paid_by`).
   - Grain: `allocation_id` (`m_share_account_charge_paid_by.id`).
   - Parameters: `office_ids` + `limit`.
   - Anti-fanout: allocation grain directly surfaces `pb.amount` as the only money measure, avoiding sum of transaction fan-outs.
   - Surfaces `share_transaction_id` and `charge_transaction_id` as evidence columns.
   - Authorization via the share account's client office (`c.office_id = ANY($1::bigint[])`).
5. `share_account_dividends` (SHARE-4):
   - Dividends declared and posted on share accounts (`m_share_account_dividend_details`).
   - Grain: `dividend_id` (`m_share_account_dividend_details.id`).
   - Parameters: `office_ids` + `limit`.
   - Status: 100=Initiated, 300=Posted (`ShareAccountDividendStatusType.java:23`).
   - Fixture fact: 0 rows in database, verified as genuine `outcome = "Empty"` with `completeness = "Complete"`.

### 20.1 Production vs independent SQL (HTTP, all authorized offices `{1,2,3,4,5,6,8,9}`)

Independent oracles are `tests/answers/{share_accounts,share_account_transactions,share_account_charges,share_charge_payment_allocations,share_account_dividends}.sql` (+ `__scope_2_3` variants).
Locked run (`answers/share`): 33 requests / tests against independent SQL oracles for all authorized offices `{1,2,3,4,5,6,8,9}` and narrowed scope `[2, 3]`, plus empty population test in office 2 (`outcome = "Empty"`).

| Capability | Window / Params | Rows (all offices) | Rows (`office_ids` [2,3]) | Job ids (all / narrowed) |
| --- | --- | --- | --- | --- |
| share_accounts | 2025-11-01..2026-04-30 | 2 | 0 | live Bruno run |
| share_account_transactions | 2025-11-01..2026-04-30 | 3 | 0 | live Bruno run |
| share_account_charges | office_ids + limit | 2 | 0 | live Bruno run |
| share_charge_payment_allocations | office_ids + limit | 2 | 0 | live Bruno run |
| share_account_dividends | office_ids + limit | 0 | 0 | live Bruno run |

In addition, office 2 empty population tested (`share_accounts`): 0 rows returned with `outcome = "Empty"`.

### 20.2 VALUES-only semantics smoke (`tests/share_semantics_smoke.sh`)

`tests/share_semantics_smoke.sh` shadows all underlying Fineract tables (`m_share_account`, `m_client`, `m_share_product`, `m_share_account_transactions`, `m_share_account_charge`, `m_charge`, `m_share_account_charge_paid_by`, `m_share_account_dividend_details`, `m_office`) using inline `VALUES` CTEs with unmodified production SQL text:
1. **Accounts Scope & Status Filtering**: Includes active share accounts in authorized office 1; filters out accounts in unauthorized office 99; filters out accounts outside submission date window; hides account numbers and PII.
2. **Transactions Scope & Date Filtering**: Includes purchase and redemption transactions within date window in authorized office; filters out transactions outside window; filters out unauthorized offices.
3. **Charges Semantics & Null Preservation**: Includes charges in authorized office; preserves NULL derived amounts; maps charge time, calculation, and payment mode enums.
4. **Allocations Anti-Fanout & Granularity**: Surfaces exact allocation rows (`m_share_account_charge_paid_by.id`); preserves link to share transaction and charge transaction without summing fan-out.
5. **Dividends Lifecycle & Status**: Surfaces initiated and posted dividend records in authorized office; filters out unauthorized offices; validates Empty semantics when no records match.
All 5 assertion checks pass cleanly.

### 20.3 CARRY-OVER rules

1. **Examples run end to end through real job execution**: Verified via locked Bruno suite `answers/share`.
2. **Prose matches SQL**: Reviewed and verified in query and capability manifests `checks` sections.
3. **Output sensitivity**: Only public business identifiers, dates, quantities, amounts, currency codes, and status strings are returned; `account_no`, `external_id`, and `client_name` are never returned.
4. **Semantics**: Explicit date windows for account submissions and transactions, client office authorization for all capabilities, anti-fanout allocations, genuine Empty outcome for dividends.

### 20.4 Inventory & Domain Gap Alignment

`docs/data/dataset-inventory.md` rows SHARE-1..6, 8 moved to `inherited` (SHARE-6, 8 shared with client/general capabilities). SHARE-7 (evidence of settled share purchase/redemption) confirmed as gap intent (`gap_intents`) in `knowledge/domains/share.yaml`.
`fineract-assistant-api/engine/policy-deferred-share-job.yml` created to assert remaining SHARE-7 gap intent ("Show settled share purchases and redemptions for this client.").

## 21. FIN-127 / SAV-1..11 — savings full §22 expansion

**Context & Scope.** `dataset-inventory.md` §6 rows SAV-1..11 cover the domain `savings`. In FIN-127, the remaining gap rows SAV-2, SAV-6, SAV-8, SAV-9, SAV-10, as well as the partial-inherited caveats on SAV-4 and SAV-5, are fully resolved by introducing 7 Mode-1 capabilities:
- `savings_account_lifecycle` (SAV-2): Full account lifecycle covering submitted/activated/closed/dormant/block dates, sub_status_enum (mapped via `SavingsAccountSubStatusEnum.java`), and savings deposit_type_enum in (100, 300, 400).
- `savings_interest_position` (SAV-6): Account-level earned, posted, and accrued interest, interest compounding/posting period enums, and nominal annual interest rate.
- `savings_account_holds` (SAV-8): Transaction-level hold balances, hold reasons, release dates, and parent hold associations (`sat.sub_resource_id = 20/21/22` or `sat.parent_hold_transaction_id`).
- `savings_withheld_tax` (SAV-9): Withholding tax percentage, tax group, and tax withholding flag on savings accounts. Genuine Empty outcome with `completeness = Complete` on standard fixture.
- `savings_linked_accounts` (SAV-10): Portfolio account associations (`m_portfolio_account_associations`) linking savings accounts to loan or other portfolio accounts.
- `savings_account_balance_detail` (SAV-4): Account-grain balance breakdown including nominal balance, available balance, on-hold funds, unauthorized overdraft, and overdraft limit.
- `savings_transactions_by_type` (SAV-5): Aggregated transaction counts, total amount, and average amount grouped strictly by `[currency_code, transaction_type_enum, transaction_type_name]` without cross-currency mixing.

**Built (Mode 1, bound `$1::bigint[]` authorized office scope, limit hard cap 200 with disclosed row cap):**
1. `savings_account_lifecycle` (SAV-2):
   - Lists savings accounts in authorized client offices (`c.office_id = ANY($1::bigint[])`).
   - Grain: `account_id` (`sa.id`).
   - Surfaces `sub_status_enum`, `sub_status_name`, `submitted_date`, `approved_date`, `activated_date`, `closed_date`, `dormant_date`, `lock_date`.
   - Identity & privacy: `client_name`, `account_no`, and `external_id` are never returned.
2. `savings_interest_position` (SAV-6):
   - Surfaces earned vs posted interest derived amounts (`total_interest_earned_derived`, `total_interest_posted_derived`), `nominal_annual_interest_rate`, compounding and posting periods.
   - Grain: `account_id` (`sa.id`).
   - Preserves distinction between earned interest and posted interest.
3. `savings_account_holds` (SAV-8):
   - Surfaces hold/release transactions on savings accounts.
   - Grain: `hold_transaction_id` (`sat.id`).
   - Maps hold amounts, release transaction dates, hold reason descriptions, and parent hold references.
4. `savings_withheld_tax` (SAV-9):
   - Surfaces withholding tax configuration and tax group identifiers on savings accounts (`sa.withhold_tax`, `tg.name`).
   - Grain: `account_id` (`sa.id`).
   - Fixture fact: 0 rows on standard test data; verified as genuine `outcome = "Empty"` with `completeness = "Complete"`.
5. `savings_linked_accounts` (SAV-10):
   - Surfaces associations between savings accounts and other portfolio accounts (`m_portfolio_account_associations`).
   - Grain: `association_id` (`paa.id`).
   - Maps association type enums (1 = Linked account association).
6. `savings_account_balance_detail` (SAV-4):
   - Surfaces granular balance components: `account_balance_derived`, `available_balance`, `on_hold_funds`, `overdraft_amount`, `overdraft_limit`.
   - Grain: `account_id` (`sa.id`).
7. `savings_transactions_by_type` (SAV-5):
   - Grouped transaction totals: `currency_code`, `transaction_type_enum`, `transaction_type_name`, `transaction_count`, `total_amount`, `avg_amount`.
   - Strict currency grouping prevents multi-currency aggregation violation (#14).

### 21.1 Production vs independent SQL (HTTP, all authorized offices `{1,2,3,4,5,6,8,9}`)

Independent oracles are `tests/answers/{savings_account_lifecycle,savings_interest_position,savings_account_holds,savings_withheld_tax,savings_linked_accounts,savings_account_balance_detail,savings_transactions_by_type}.sql` (+ `__scope_2_3` variants).
Locked run (`answers/savings`): 42 new requests / tests against independent SQL oracles for all authorized offices `{1,2,3,4,5,6,8,9}` and narrowed scope `[2, 3]`, plus empty population tests (`outcome = "Empty"`).

| Capability | Window / Params | Rows (all offices) | Rows (`office_ids` [2,3]) | Job ids (all / narrowed) |
| --- | --- | --- | --- | --- |
| savings_account_lifecycle | office_ids + limit | 187 | 81 | live Bruno run |
| savings_interest_position | office_ids + limit | 187 | 81 | live Bruno run |
| savings_account_holds | office_ids + limit | 23 | 0 (Empty) | live Bruno run |
| savings_withheld_tax | office_ids + limit | 0 (Empty) | 0 (Empty) | live Bruno run |
| savings_linked_accounts | office_ids + limit | 90 | 31 | live Bruno run |
| savings_account_balance_detail | office_ids + limit | 187 | 81 | live Bruno run |
| savings_transactions_by_type | office_ids + limit | 33 | 20 | live Bruno run |

### 21.2 VALUES-only semantics smoke (`tests/savings_semantics_smoke.sh`)

`tests/savings_semantics_smoke.sh` shadows all underlying Fineract tables (`m_savings_account`, `m_client`, `m_savings_product`, `m_savings_account_transaction`, `m_portfolio_account_associations`, `m_tax_group`, `m_office`) using inline `VALUES` CTEs with unmodified production SQL text:
1. **Lifecycle Scope & Status Filtering**: Covers submitted, approved, active, closed, and dormant/block status mapping; filters out fixed/recurring deposits (`deposit_type_enum = 200`); filters out unauthorized offices.
2. **Interest Position Calculations**: Verifies distinct earned vs posted interest; checks compounding and posting period enums.
3. **Account Holds & Releases**: Verifies hold amounts, release amounts, parent hold relationships, and office boundary enforcement.
4. **Withheld Tax Scope & Empty Semantics**: Verifies tax group join and Empty outcome when no tax withholding exists.
5. **Linked Accounts Associations**: Verifies association types, linked account IDs, active flags, and office boundaries.
6. **Balance Detail Granularity**: Verifies available balance, on-hold funds, overdraft amounts, and overdraft limits.
7. **Transactions by Type Aggregation**: Verifies grouping by currency code and transaction type enum without cross-currency contamination.
All 7 assertion checks pass cleanly.

### 21.3 CARRY-OVER rules

1. **Examples run end to end through real job execution**: Verified via locked Bruno suite `answers/savings`.
2. **Prose matches SQL**: Reviewed and verified in query and capability manifests `checks` sections.
3. **Output sensitivity**: Only public business identifiers, dates, quantities, amounts, currency codes, and status strings are returned; `account_no`, `external_id`, and `client_name` are never returned.
4. **Semantics**: Office authorization via client office, proper enum mappings to Fineract source enums, multi-currency safe grouping.

### 21.4 Inventory & Domain Gap Alignment

`docs/data/dataset-inventory.md` rows SAV-2, SAV-4, SAV-5, SAV-6, SAV-8, SAV-9, SAV-10, FDRD-5, FDRD-7 moved to `inherited`. Domain `savings` now has 0 gap rows across all 11 items. Domain `tax` updated to `approved_mvp`.

## 22. FIN-128 / CLI-1..8 — client/ownership full §20 expansion

**Context & Scope.** `dataset-inventory.md` §4 rows CLI-1..8 cover domain `client` (Client/kepemilikan, §1 baseline: identity, lifecycle, assignment, group/center, and all account types). Prior to FIN-128, CLI-1..3 and D04 charges were approved Mode-1 capabilities, while CLI-4, CLI-5, and CLI-6 remained gap rows; CLI-7 (identity documents) and CLI-8 (notes) are permanently excluded by design. In local `fineract_default`:
- `m_group`: 2 groups (`level_id=2` group in office 3, `level_id=1` center in office 2). `parent_id` is NULL on all rows (center hierarchy not populated in live fixture, linkage is NULL-safe).
- `m_group_client`: 3 membership rows (clients 4, 5, 9 in office 3, all members of group 1). Office 2 has center 2 with 0 members. Cardinality `N:M` client<->group.
- `m_client_address`: 1 row live (client 73, office 4). Offices 2 & 3 have 0 address rows (genuine Empty on narrowed scope `[2,3]`).
- Roster across account types: 116 loans (`m_loan`), 207 savings accounts (`m_savings_account`), 2 share accounts (`m_share_account`). Total union rows = 325 (capped at 200); narrowed `[2,3]` = 121 rows.

**Built (Mode 1, bound `$1::bigint[]` authorized office scope, limit hard cap 200 with disclosed row cap):**
1. `client_group_membership` (CLI-4):
   - Lists client group and center memberships: client ID, client office ID, group ID, group name, level ID, parent ID (center linkage), and group office ID.
   - Grain: `[client_id, group_id]` (`m_group_client.client_id`, `m_group_client.group_id`).
   - Authorization: governed by the client's office (`m_client.office_id = ANY($1::bigint[])`); group office surfaced as evidence only.
   - Center linkage is NULL-safe (group 1 has `parent_id = NULL` live).
2. `client_account_roster` (CLI-5):
   - Per-client UNION ALL across `m_loan`, `m_savings_account`, `m_share_account`.
   - Grain: `[account_type, account_id]`.
   - Account type surfaced as declared literal (`'loan'`, `'savings'`, `'share'`).
   - Raw status enum preserved: loan `loan_status_id` (100/200/300/400/500/600/601/602/700 via `r_enum_value`), savings `status_enum` (`SavingsAccountStatusType.java:29-39`), share `status_enum` (`ShareAccountStatusType.java:26-31`).
   - No default-active filter (§1 rule 34: all accounts owned by client surfaced).
   - Authorization: loan via client office (`m_loan.group_id -> m_group.office_id` path declared, 0 live rows), savings/share via client office.
3. `client_address` (CLI-6):
   - Client contact and address details: joins `m_client_address` -> `m_client`, `m_address`, and `m_code_value`.
   - Grain: `[client_address_id]` (`m_client_address.id`).
   - Selective: triggers only when address/contact is explicitly asked.
   - Address fields carry `pii` sensitivity classification (gated behind `can_view_pii` / PII withholding machinery).
   - Authorization: governed by client office (`m_client.office_id = ANY($1::bigint[])`).

### 22.1 Production vs independent SQL (HTTP, all authorized offices `{1,2,3,4,5,6,8,9}`)

Independent oracles are `tests/answers/{client_group_membership,client_account_roster,client_address}.sql` (+ `__scope_2_3` variants).
Locked run (`answers/client`): all requests tested against independent SQL oracles for all authorized offices `{1,2,3,4,5,6,8,9}` and narrowed scope `[2, 3]`, plus genuine Empty population tests in office 2 (group membership) and office 9 (account roster).

| Capability | Params | Rows (all offices) | Rows (`office_ids` [2,3]) | Job ids (all / narrowed) |
| --- | --- | --- | --- | --- |
| client_group_membership | office_ids + limit | 3 | 3 | live Bruno run |
| client_account_roster | office_ids + limit | 200 (capped from 325) | 121 | live Bruno run |
| client_address | office_ids + limit | 1 | 0 (Empty) | live Bruno run |

In addition, empty population tested:
- `client_group_membership`: office 2 returns 0 rows with `outcome = "Empty"`.
- `client_account_roster`: office 9 returns 0 rows with `outcome = "Empty"`.
- `client_address`: offices `[2,3]` return 0 rows with `outcome = "Empty"`.

### 22.2 VALUES-only semantics smoke (`tests/client_semantics_smoke.sh`)

`tests/client_semantics_smoke.sh` shadows all underlying Fineract tables (`m_client`, `m_group`, `m_group_client`, `m_client_address`, `m_loan`, `m_savings_account`, `m_share_account`, `m_office`, `m_address`, `m_code_value`, `m_loan_product`, `m_savings_product`, `m_share_product`) using inline `VALUES` CTEs with unmodified production SQL text:
1. **Group Membership N:M & Center Linkage**: Client 1 in group 10 (center 20) and group 11 (NULL center); client 2 in group 10; unauthorized client 3 in office 9 filtered.
2. **Account Roster Multi-Type & No Default Active**: Surfaces loan, savings, and share account types; submitted/pending accounts (status 100) surfaced alongside active accounts (status 300); unauthorized client 3 in office 9 filtered.
3. **Address Details & Authorization**: Address 6001 surfaced with street, city, postal code, and type RESIDENTIAL; unauthorized client 3 address in office 9 filtered.
4. **Empty Semantics**: Querying empty office scope returns 0 rows across all capabilities.
All assertion checks pass cleanly.

### 22.3 CARRY-OVER rules

1. **Examples run end to end through real job execution**: Verified via locked Bruno suite `answers/client`.
2. **Prose matches SQL**: Reviewed and verified in query and capability manifests `checks` sections.
3. **Output sensitivity**: Address fields classified as `pii`; account numbers and external IDs are never returned across all capabilities.
4. **Semantics**: Client-office authorization, no default-active filter, center linkage NULL-safe, genuine Empty outcomes.

### 22.4 Inventory & Domain Gap Alignment

`docs/data/dataset-inventory.md` rows CLI-4, CLI-5, and CLI-6 moved to `inherited` (Mode-1 capabilities). CLI-7 (identity documents) and CLI-8 (notes) remain excluded by design.
`fineract-assistant-api/engine/policy-deferred-client-address-job.yml` updated to assert remaining client gap intent: `family member detail` (D15d).

## 23. FIN-130 / LINK-3..8 — linking_resources (§25 resource penghubung) full expansion

**Context & Scope.** `dataset-inventory.md` §9 rows LINK-3..8 cover domain `linking_resources` (Resource penghubung, `dataset-scope-decisions.md` §30, binding rules lines 32–38: linked accounts, transfers, allocation, settlement, reversal, payment type). Prior to FIN-130, domain `linking_resources` was in `status: gap` with all six capabilities deferred. FIN-130 transitions `linking_resources` to `status: approved_mvp` by implementing six curated Mode-1 capabilities. In local `fineract_default`:
- `m_portfolio_account_associations`: 90 rows total, all `association_type_enum = 1` (`LINKED_ACCOUNT_ASSOCIATION`, `AccountAssociationType.java:26-28`; 0 rows of type 0 or 2 — confirms FIN-127). Office spread through the delivered view's NULL-safe client path (any of the four account sides): 1(2), 2(2), 3(29), 4(26), 5(14), 6(10), 8(7).
- `m_account_transfer_details`: 498 rows.
- `m_account_transfer_transaction`: 728 rows by `from_office_id`: 1(3), 2(156), 3(197), 4(158), 5(93), 6(75), 8(46). Population exceeds default hard cap 200.
- `m_savings_account_charge_paid_by`: 165 rows by office of the paying savings transaction: 1(1), 2(2), 3(76), 4(81), 5(2), 6(1), 8(2). The loan side has no paid-by table (`m_loan_charge.amount_paid_derived` is a derived total, LOAN-7).
- `m_payment_detail`: 489 rows across offices 1, 2, 3. Re-verified count on live database is 489 rows.
- Reversals: 88 reversed loan transactions (1:7, 2:3, 3:60, 4:12, 5:1, 6:5) and 6,788 reversed/reversal savings transactions (2:8, 3:6,483, 4:284, 5:10, 6:3); 0 reversed `acc_gl_journal_entry` rows. Fineract schema for `m_share_account_transactions` has no reversal field or reversal transaction type — share reversals are absent by data model (SHARE-8).
- Standing instructions (D05): excluded and deferred under FIN-114.

**Built (Mode 1, bound `$1::bigint[]` authorized office scope, limit hard cap 200 with disclosed row cap):**
1. `account_association_links` (LINK-3):
   - Lists portfolio account associations (savings to loan, savings to savings, and loan-side links; only type 1 rows exist live).
   - Grain: `[association_id]` (`m_portfolio_account_associations.id`).
   - Authorization: any of the four account sides' client office, NULL-safe (`COALESCE(cl.office_id, cs.office_id, cls.office_id, cll.office_id)` in SQL).
   - Enums: Association type decoded via `AccountAssociationType.java:26-28`.
2. `account_transfer_transactions` (LINK-4):
   - Operational account transfer transactions between savings/loan accounts.
   - Grain: `[transfer_transaction_id]` (`m_account_transfer_transaction.id`); details:transactions is 1:N, never fanned out.
   - Population: 728 rows (hits 200 row cap on all offices; 353 on scope [2,3] hitting 200 row cap; office 9 returns genuine Empty outcome).
   - Authorization: `from_office_id` on the transfer detail header (owner side); `to_*` columns are evidence only.
3. `loan_payment_allocation` (LINK-5):
   - Allocation of one loan transaction across principal, interest, fee, and penalty portion columns; savings transactions are single-purpose (stated, not faked).
   - Grain: `[loan_transaction_id]` (`m_loan_transaction.id`).
   - Population: 2,947 rows (hits 200 row cap on all offices; 951 rows on scope [2,3], capped as well). Allocation-consistency contract: components sum to `amount`.
   - Authorization: loan office (`lt.office_id = ANY($1::bigint[])`).
4. `charge_settlement_evidence` (LINK-6):
   - Savings charge-settlement evidence linking `m_savings_account_charge` to its paying `m_savings_account_transaction`; settlement is never inferred from a balance drop.
   - Grain: `[charge_paid_by_id]` (`m_savings_account_charge_paid_by.id`).
   - Rows: 165 total (78 in scope [2,3]; office 9 returns genuine Empty outcome).
   - Authorization: office of the paying transaction (`sat.office_id = ANY($1::bigint[])`). Loan side stated as derived totals only (`m_loan_charge.amount_paid_derived`, LOAN-7); no per-transaction settlement link exists there.
5. `transaction_reversals` (LINK-7):
   - Cross-portfolio reversal flags and original↔reversal linkage across loan transactions, savings transactions, and GL journal entries.
   - Grain: `[entity_type, transaction_id]`.
   - Rows: 6,876 total (200-row cap; 6,554 on scope [2,3], capped).
   - Discriminator: `'loan'`, `'savings'`, `'gl'`. GL rows expose only the reversal flag and `reversal_id` reference — no D12 accounting capability is claimed (balances/mutations stay gap). Share accounts have no reversal field (stated explicitly).
   - Authorization: governed by entity transaction office (`lt.office_id`, `sat.office_id`, `gl.office_id`).
6. `payment_type_resolution` (LINK-8):
   - Payment type name and cash flag resolved per transaction via `m_payment_detail` → `m_payment_type`; NULL `payment_detail_id` is NULL-safe (rows preserved).
   - Grain: `[account_type, transaction_id]` (UNION ALL over loan and savings transactions).
   - Population: 18,554 rows = 2,947 loan + 15,607 savings (hits 200 row cap on all offices; 13,337 on scope [2,3], capped).
   - Authorization: governed by transaction office (`lt.office_id`, `sat.office_id`).
   - PII exclusion: `m_payment_detail` fields (account number, routing number, check number) and `account_no`/`external_id` are never selected.

### 23.1 Production vs independent SQL (HTTP, all authorized offices `{1,2,3,4,5,6,8,9}`)

Independent oracles are `tests/answers/{account_association_links,account_transfer_transactions,loan_payment_allocation,charge_settlement_evidence,transaction_reversals,payment_type_resolution}.sql` (+ `__scope_2_3` and `__population` variants).
Locked run (`answers/linking_resources`): all requests tested against independent SQL oracles for all authorized offices `{1,2,3,4,5,6,8,9}` and narrowed scope `[2, 3]`, plus genuine Empty population tests in office 9 (transfer transactions and charge settlements).

| Capability | Params | Rows (all offices) | Rows (`office_ids` [2,3]) | Job outcome (all / narrowed) |
| --- | --- | --- | --- | --- |
| account_association_links | office_ids + limit | 90 | 31 | Completed / Answered |
| account_transfer_transactions | office_ids + limit | 200 (capped from 728) | 200 (capped from 353) | Completed / Answered |
| loan_payment_allocation | office_ids + limit | 200 (capped from 2947) | 200 (capped from 951) | Completed / Answered |
| charge_settlement_evidence | office_ids + limit | 165 | 78 | Completed / Answered |
| transaction_reversals | office_ids + limit | 200 (capped from 6876) | 200 (capped from 6554) | Completed / Answered |
| payment_type_resolution | office_ids + limit | 200 (capped from 18554) | 200 (capped from 13337) | Completed / Answered |

In addition, empty population tested:
- `account_transfer_transactions`: office 9 returns 0 rows with `outcome = "Empty"`.
- `charge_settlement_evidence`: office 9 returns 0 rows with `outcome = "Empty"`.

### 23.2 VALUES-only semantics smoke (`tests/linking_resources_semantics_smoke.sh`)

`tests/linking_resources_semantics_smoke.sh` shadows all underlying Fineract tables (`m_portfolio_account_associations`, `m_account_transfer_details`, `m_account_transfer_transaction`, `m_payment_detail`, `m_payment_type`, `m_loan`, `m_loan_transaction`, `m_savings_account`, `m_savings_account_charge`, `m_savings_account_charge_paid_by`, `m_savings_account_transaction`, `acc_gl_journal_entry`, `m_client`, `m_office`) using inline `VALUES` CTEs with unmodified production SQL text:
1. **Association Type Mapping & Office Boundaries**: Decodes `LINKED_ACCOUNT_ASSOCIATION` (1) and `GUARANTOR_ACCOUNT_ASSOCIATION` (2); filters unauthorized office 9.
2. **Transfer Transactions Office Boundary & Filtering**: Enforces authorization on source account office; unauthorized transfers filtered.
3. **Loan Payment Allocation Breakdown**: Accurately separates principal, interest, fees, and penalties per installment; enforces loan office scope.
4. **Charge Settlement Evidence**: Links savings charges to their paying transactions; verifies the charge amount on the paid-by row.
5. **Multi-Entity Transaction Reversals**: Combines reversed loan, savings, and GL journal-entry rows; verifies exclusion of unreversed transactions and unauthorized offices.
6. **Payment Type Resolution & Breakdown**: Resolves payment type names and breakdown without leaking sensitive payment details; enforces office boundary.
7. **Empty Semantics**: Querying unauthorized or nonexistent office scope returns 0 rows across all capabilities.
All 10 assertion checks pass cleanly.

### 23.3 CARRY-OVER rules

1. **Examples run end to end through real job execution**: Verified via locked Bruno suite `answers/linking_resources`.
2. **Prose matches SQL**: Reviewed and verified in query and capability manifests `checks` sections.
3. **Output sensitivity**: Only operational IDs, dates, quantities, amounts, currency codes, and status strings returned; `account_no`, `external_id`, routing/check numbers, and client names are never returned.
4. **Semantics**: Office authorization bound via `$1::bigint[]`, proper enum mappings to Fineract source enums, row cap 200 with population metadata.

### 23.4 Inventory & Domain Gap Alignment

`docs/data/dataset-inventory.md` rows LINK-3, LINK-4, LINK-5, LINK-6, LINK-7, and LINK-8 moved to `inherited` (Mode-1 capabilities).
Domain `linking_resources` moved to `status: approved_mvp` with 0 gap rows.
Standing instructions (D05 / FIN-114) remain excluded/deferred.
