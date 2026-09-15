# Kontrak response document

Status: disepakati 2026-09-13. Menutup utang **D1–D3** pada [database-design.md](../data/database-design.md) §2.2. Penyimpanan dimiliki `job_responses` (#10); transport dimiliki [api.md](api.md) dan [sse.md](sse.md); lifecycle dimiliki [engine.md](../architecture/engine.md). Skema JSON penuh per blok adalah pekerjaan OpenAPI di `api.md`, bukan dokumen ini.

> **Status implementasi (2026-09-15).** Sudah dipancarkan: `metrics`, `table`,
> `narrative`, `provenance`, `limitation` — seluruhnya deterministik, tanpa
> model. **D1–D3 ditegakkan runtime** di `crates/chat/src/engine/validate.rs`:
> setiap response `analysis` dihitung ulang terhadap ledger sebelum di-commit,
> dan yang gagal digantikan fallback deterministik (§6). **Belum**: `chart`,
> `findings`, `comparison`, `suggestions`, narasi LLM, dan entri `derivation`
> (validator sudah menerimanya, tetapi belum ada yang memproduksinya karena
> `evidence_json` belum disusun). Blok yang benar-benar dikirim ada di
> [api-reference.md §5](api-reference.md#5-job); status lengkap di
> [status.md](../status.md).
>
> Dua hal yang dipersempit dari tulisan di bawah, dan sengaja:
>
> - **Daftar pengecualian D3 (§4) tidak dibuat sebagai daftar.** Yang berlaku:
>   numeral prosa wajib cocok dengan angka pada blok ber-evidence (`metrics`,
>   `table`, `comparison`, `provenance`) atau dengan `result` sebuah
>   `derivation`. Karena `provenance` memuat parameter yang terikat, tahun pada
>   periode dan nilai identitas yang disalin apa adanya sudah tergrounding
>   tanpa aturan khusus — dan tidak ada daftar yang perlu dijaga tetap sinkron.
> - **Blok `limitation` tidak diperiksa dan tidak membuktikan apa pun.** Ia
>   prosa server yang deterministik; memeriksanya tidak menangkap apa pun, dan
>   menjadikannya evidence akan mengesahkan angka narasi hanya karena kebetulan
>   muncul di kalimat limitation.

## Prinsip

> **Response bukan hasil yang dipercaya, melainkan hasil yang dihitung ulang dan ditolak bila tidak cocok.**

Setiap aturan di dokumen ini berbentuk pemeriksaan yang dapat dijalankan mesin terhadap data yang sudah durable (`job_node_runs`, `datasets`, `clarification_answers`, `chat_jobs.scope_json`). Tidak ada aturan yang bergantung pada niat baik composer. Ini konsekuensi langsung dari invarian **I6**: hand-off antar tahap ditegakkan pemeriksaan, bukan konvensi.

Data bersifat otoritatif; narasi LLM bersifat aditif dan wajib tergrounding evidence (PRD §9).

---

## 1. Struktur dokumen

Satu response document = `job_responses` satu baris (immutable, berversi).

Setiap blok di `blocks_json` membawa:

| Field | Wajib | Keterangan |
| --- | --- | --- |
| `block_id` | ya | stabil di dalam satu `response_version` |
| `type` | ya | lihat §2 |
| `schema_version` | ya | **per blok**, bukan hanya per dokumen |
| `derived_from` | ya untuk blok penyaji data | daftar `node_run_id` dan/atau `dataset_id` |
| payload | ya | spesifik per tipe |

`schema_version` per blok agar menambah tipe blok baru tidak memaksa menaikkan versi seluruh dokumen.

**Kompatibilitas klien**: tipe blok yang tidak dikenal **dilewati tanpa merusak render**, dan server tidak pernah bergantung pada klien merendernya. Konsekuensinya, informasi yang wajib sampai (limitation, pengungkapan auto-bind, PII ditahan) **tidak boleh** hanya hidup di tipe blok baru.

---

## 2. Kosakata blok

| Tipe | Isi | `derived_from` |
| --- | --- | --- |
| `narrative` | prosa penjelas, aditif | wajib bila memuat angka |
| `metric` | satu nilai bernama + unit + periode | wajib |
| `table` | kolom + baris kecil **atau** rujukan dataset handle + pagination | wajib |
| `chart_spec` | spesifikasi chart atas data yang sudah ada | wajib |
| `comparison` | dua atau lebih nilai sebanding + selisih | wajib |
| `finding` | temuan, dibedakan fakta vs hipotesis | wajib |
| `limitation` | apa yang tidak terjawab dan kenapa | tidak |
| `suggestion` | analisis lanjutan yang didukung | tidak |
| `note` | pengungkapan asumsi/binding/presedensi | tidak |

**Tabel besar tidak pernah disalin inline** — blok `table` merujuk dataset handle (#10, #11). Hasil kecil boleh inline (#11 aturan 3).

---

## 3. D1 — `completeness` dihitung, bukan dipilih

Urutan keparahan:

```
Complete  <  Partial  <  Unknown
```

`Unknown` paling buruk: "tidak tahu apakah lengkap" lebih berbahaya daripada "tahu tidak lengkap".

### Aturan

1. Setiap blok penyaji data wajib memiliki `derived_from`.
2. `completeness` blok = **terburuk** di antara seluruh kontributornya (`job_node_runs.completeness` dan `datasets.completeness`).
3. `job_responses.completeness` = **terburuk** di antara seluruh blok.
4. Validator **menghitung ulang** nilai (2) dan (3) dari ledger, lalu membandingkannya dengan klaim composer.

### Arah yang diizinkan

| Klaim composer vs hitungan | Hasil |
| --- | --- |
| lebih buruk | **diterima** — composer boleh tahu celah yang tidak terlihat di data node |
| sama | diterima |
| **lebih baik** | **DITOLAK** → `validation_status='failed'` |

Satu arah, dan itulah yang membuat `Complete` di atas data `Partial` mustahil lolos — bukan karena disiplin, melainkan karena dihitung ulang.

`completeness_reason` wajib terisi bila hasilnya bukan `Complete`, dan wajib menyebut kontributor yang menyebabkannya.

---

## 4. D3 — setiap angka wajib berdasar

**Narasi tidak boleh memperkenalkan angka yang tidak berdasar.**

### Pemeriksaan

Validator mengekstrak numeral dari seluruh blok `narrative`, `finding` dan `comparison`, lalu mencocokkan setiap numeral dengan:

1. nilai pada blok `metric`/`table`/`comparison` di dokumen yang sama, dengan toleransi format (pemisah ribuan, simbol mata uang, pembulatan tampilan yang dideklarasikan); **atau**
2. entri `derivation` pada `evidence_json`.

Numeral yang tidak cocok keduanya → **validasi gagal**.

### `derivation`

Angka turunan yang wajar untuk prosa ("naik 12%", "rata-rata 3,4 juta") tidak ada di blok mana pun. Ia **boleh** dipakai hanya bila dideklarasikan:

```
derivation: { id, formula, inputs: [block_id/metric refs], result, rounding }
```

Tidak dideklarasikan → ditolak.

**Konsekuensi yang diterima secara sadar**: aturan ini akan menolak narasi yang sebenarnya benar tetapi malas mendeklarasikan turunannya. Itu harga yang tepat — kegagalannya keras dan terlihat, bukan angka salah yang lolos diam-diam.

### Dikecualikan dari pemeriksaan

Numeral yang jelas bukan klaim data: nomor urut blok, tahun dalam nama periode yang sudah muncul di parameter, dan angka di dalam label yang disalin apa adanya dari evidence. Daftar pengecualian bersifat tertutup dan direview bersama migrasi pertama.

### Evidence lineage

`evidence_json` merekam rantai **finding → metric → operasi → dataset → sumber**: `node_run_id`, `dataset_id`, capability/analytical-contract + versi, `catalog_version_id` + `catalog_content_hash`, `as_of`, dan `exchange_rate_id` bila ada konsolidasi mata uang (#14).

---

## 5. D2 — auto-bind wajib diungkap

Untuk setiap slot yang diikat dengan provenance `resolver_unique` atau `deterministic_parse` (K5), response wajib memuat pengungkapan pada blok `note`: nama slot, nilai yang diikat, dan alasannya.

Contoh: "Rekening tabungan **SA-000123** — satu-satunya yang cocok." · "Periode: **1–30 September 2026** (dari 'bulan ini')."

**Pemeriksaan**: validator membandingkan himpunan slot auto-bind dari `job_node_runs.input_binding_json` dengan himpunan slot yang diungkap. Tidak sama → validasi gagal.

Auto-bind yang diam adalah cara menghasilkan jawaban salah dengan meyakinkan; karena itu pemeriksaannya mekanis, bukan penilaian.

---

## 6. Validasi dan fallback

| `validation_status` | Arti |
| --- | --- |
| `passed` | disajikan |
| `failed` | ditolak; **tetap disimpan** sebagai bahan investigasi (#10) |
| `fallback` | versi konservatif yang disajikan menggantikan versi `failed` |

### Fallback wajib deterministik

Versi fallback **tidak boleh dihasilkan LLM**. Jalur kegagalan tidak boleh bergantung pada komponen yang barusan gagal.

Fallback disusun secara deterministik: blok yang lolos pemeriksaan dipertahankan apa adanya, blok yang gagal dibuang, dan satu blok `limitation` menyebutkan apa yang dibuang beserta alasannya. `completeness` dihitung ulang atas blok yang tersisa (§3).

Bila setelah pembuangan tidak ada blok data yang tersisa, hasilnya adalah response `kind='limitation'` — bukan dokumen kosong, dan bukan job yang gagal diam-diam.

---

## 7. Aturan lain

**Blok yang gagal diproduksi** (node gagal, fail-policy mengizinkan lanjut) menjadi blok `limitation` eksplisit dan ikut memperburuk `completeness`. Tidak pernah hilang diam-diam (**I5**).

**Suggestion bukan aksi.** Membawa label + `prefill`, bukan binding yang dapat dieksekusi. Memilihnya membuat **job baru dengan otorisasi baru**; tidak ada jalur resume dari suggestion.

**Presedensi permintaan format/field admin**:

```
1. parameter request terstruktur
2. jawaban klarifikasi
3. teks natural
```

Konflik antar-tingkat **wajib diungkap** pada blok `note`, tidak pernah diselesaikan diam-diam.

**Chart**: `chart_spec` mendeklarasikan bentuk data yang dibutuhkan (mis. time series memerlukan dimensi waktu terurut). Validator memeriksa data rujukannya memenuhi bentuk itu. Tidak kompatibel → **turun menjadi `table` + `note`**; tidak pernah merender chart yang menyesatkan.

**PII dimatikan (#15)**: blok `table` mendeklarasikan kolom yang ditahan, dan blok `limitation` menyatakannya. Validator memeriksa konsistensi dengan `chat_jobs.scope_json`: bila `pii.enabled=false` tetapi ada kolom berkelas `pii` di output → validasi gagal.

**Handle kedaluwarsa**: blok `table` yang dataset-nya `expired`/`purged` menyatakan "detail data sudah kedaluwarsa" dan pertanyaan lanjutan menjadi retrieval baru (#11 aturan 6).

**Skip (K2)**: `kind='skipped'`, memuat hasil parsial yang valid beserta gap eksplisit; seluruh aturan validasi di atas tetap berlaku.

---

## 8. Skenario acceptance

- Response `Complete` ditolak bila salah satu node kontributornya `Partial`.
- Narasi dengan angka yang tidak ada di blok mana pun dan tanpa `derivation` ditolak.
- Angka turunan ber-`derivation` diterima dan lineage-nya dapat ditelusuri.
- Slot auto-bind yang tidak diungkap menyebabkan validasi gagal.
- Versi `failed` tetap tersimpan dan dapat diinvestigasi setelah fallback disajikan.
- Fallback tidak memanggil model.
- Chart yang tidak kompatibel turun menjadi tabel, bukan gagal dan bukan menyesatkan.
- PII dimatikan: kolom identitas tidak muncul dan penahanannya dinyatakan.
- Dataset kedaluwarsa: tabel menyatakan detail tidak lagi tersedia, angka ringkas tetap terbaca dengan `as_of`.
- Tipe blok yang tidak dikenal klien dilewati tanpa merusak render.

---

## 9. Terbuka

- Daftar pengecualian numeral (§4) final — direview bersama migrasi pertama.
- Toleransi pembulatan tampilan per kelas measure — menunggu `data/analytical-contracts.md`.
- Skema JSON penuh per blok — `contracts/api.md` (OpenAPI).
- Batas ukuran `blocks_json` dan retensi versi `failed` — [runtime.md](../operations/runtime.md).
