# Status: DIBAWA APA ADANYA, BELUM DIREVIEW

69 file SQL ini disalin dari repo `ai_report` pada 2026-09-13 **tanpa perubahan**.

Berpasangan dengan [knowledge/CARRY-OVER.md](../knowledge/CARRY-OVER.md); baca keduanya.

## Yang belum diverifikasi

- **Office scope sebagai parameter terikat.** Penegakan scope wajib berada **di dalam** SQL lewat parameter terikat, tidak pernah sebagai filter di Rust setelah fetch ([database-design.md](../docs/data/database-design.md) §2.2 **D5**). Setiap query harus diperiksa apakah benar-benar menerima dan memakai `office_ids`.
- **Dua kelas timeout.** Desain baru memisahkan `PROBE_QUERY_TIMEOUT_MS` (3 s) dari `ANALYTICAL_QUERY_TIMEOUT_MS` (15 s). Query di sini ditulis di bawah satu timeout 3 detik; sebagian query agregat kemungkinan besar melewatinya pada data berukuran produksi.
- **Grain dan anti-fanout.** `datasets.grain_json` (#11) mengharuskan grain hasil dinyatakan eksplisit. Query dengan join yang menggandakan baris harus dinyatakan grainnya atau diperbaiki.
- **Kontrak kolom output** belum dicocokkan dengan aturan validasi response ([responses.md](../docs/contracts/responses.md)), khususnya penamaan kolom yang menjadi `metric` dan `table`.
- **Semantik mata uang.** Query yang mengonsolidasi lebih dari satu mata uang wajib memakai `exchange_rates` dan merekam `exchange_rate_id` (#14). Tidak boleh ada konversi dengan kurs yang ditanam di dalam query.

## Aturan

`PREPARE` yang berhasil hanya membuktikan sintaks dan keberadaan kolom. Ia **tidak** membuktikan angkanya benar, grainnya benar, atau scope-nya ditegakkan. Jalankan contohnya ujung ke ujung dan periksa hasilnya sebelum sebuah query dianggap approved.
