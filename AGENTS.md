# Kontrak kerja untuk agen AI

Baca ini sebelum mengubah apa pun. Dokumen ini singkat karena aturannya sedikit tetapi mengikat.

## Kenapa dokumen ini ada

Sistem pendahulu gagal bukan karena satu bug besar, melainkan karena **tiap bagian diperbaiki sendiri-sendiri**: memory diperbaiki tanpa menyentuh job, klarifikasi diperbaiki tanpa menyentuh memory, dan tidak ada yang gagal ketika keduanya menyimpang. Seluruh desain di sini dibangun untuk membuat penyimpangan semacam itu **gagal keras**.

Jangan pulihkan pola lama itu.

## Sebelum menulis kode

1. Baca [docs/data/database-design.md](docs/data/database-design.md) **§1 Invarian** dan **§2 Matriks koneksi**.
2. Bila perubahan Anda menyentuh salah satu hand-off di §2, periksa penegaknya masih berlaku.
3. Bila menambah FK, periksa **§5 matriks referential action** — lihat invarian I2.

## Delapan invarian

1. **Tidak ada panggilan eksternal di dalam transaksi commit.** Audit ditulis di dalam transaksi (tulisan lokal); summary memori di luar (butuh LLM).
2. **Setiap referential action diperiksa terhadap rantai CASCADE session.** FK antar sesama anak CASCADE tidak boleh `RESTRICT`; `ON DELETE SET NULL` menuju tabel append-only dilarang.
3. **Sequence dialokasikan lewat row lock**, bukan sequence PostgreSQL (sequence meninggalkan lubang saat rollback).
4. **Ketidakpastian terlihat di data.** `Abandoned` ≠ `Failed`; `completeness` terpisah dari `status`; `row_count_total = NULL` berarti tidak diketahui, bukan nol.
5. **Tidak ada penghilangan senyap.** Truncation, handle kedaluwarsa, kurs tidak tersedia, PII dimatikan, slot auto-bind, skip — semuanya wajib dinyatakan pada response.
6. **Hand-off antar tahap ditegakkan constraint, bukan konvensi.** Bila sebuah koneksi hanya hidup sebagai aturan prosa, catat sebagai utang di §2.2 — jangan anggap selesai.
7. **Otorisasi tidak pernah dibaca dari state turunan.** Summary dan session memory bukan dasar izin.
8. **Audit hidup lebih lama daripada isinya dan tidak punya FK.** `job_id` menggantung setelah purge adalah **normal** — jangan "perbaiki" menjadi FK.

## Batasan struktural

- **Tiga crate, dan tetap tiga**: `app`, `core`, `chat`. Nama singkat, tanpa awalan `ai_report_*`.
- `route → service → repository → database`. **Tidak ada `sqlx` di handler atau service** — hanya di repository.
- Semua response HTTP memakai envelope `{ success, data, error }`. Error publik tersanitasi: SQL, prompt, dan stack tidak pernah bocor.
- **Read-only terhadap Fineract.** Jangan pernah menulis ke sana, jangan pernah mengubah schema-nya.
- Schema hanya berubah lewat `migrations/*.sql`. **Startup aplikasi tidak pernah membuat atau mengubah tabel.**
- Eksekusi terbatas pada capability yang disetujui di `knowledge/` dan `queries/` — bukan SQL yang dikarang model.

## Menjalankan pemeriksaan

```bash
cargo check --workspace
cargo clippy --workspace -- -D warnings
psql -v ON_ERROR_STOP=1 -d "$APP_DATABASE_URL" -f tests/schema_smoke.sql
```

`tests/schema_smoke.sql` menguji **perilaku**, bukan sekadar DDL berhasil di-parse: ia gagal bila K4, fencing, constraint satu-job-per-session, atau rantai CASCADE rusak. Jalankan setelah setiap perubahan migrasi.

## Yang mudah salah

- `knowledge/` dan `queries/` **dibawa apa adanya dan belum direview** — baca `CARRY-OVER.md` di masing-masing folder. Memuat YAML dan `PREPARE` SQL membuktikan sintaks, **bukan** kebenaran.
- Angka di `docs/operations/runtime.md` adalah **nilai awal**, bukan hasil tuning. Setiap angka punya pemicu revisi terukur; jangan ubah tanpa memenuhi pemicunya.
- Beberapa parameter saling terikat dan **tidak boleh diubah sendiri-sendiri** — lihat §10 pemeriksaan konsistensi di dokumen itu, terutama K1, K11 dan K12.
- Jangan menyalin `migrations/` atau kode dari repo lama. Schema-nya berbeda fundamental.
