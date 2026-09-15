# Kontrak kerja untuk agen AI

Baca ini sebelum mengubah apa pun. Dokumen ini singkat karena aturannya sedikit tetapi mengikat.

## Kenapa dokumen ini ada

Sistem pendahulu gagal bukan karena satu bug besar, melainkan karena **tiap bagian diperbaiki sendiri-sendiri**: memory diperbaiki tanpa menyentuh job, klarifikasi diperbaiki tanpa menyentuh memory, dan tidak ada yang gagal ketika keduanya menyimpang. Seluruh desain di sini dibangun untuk membuat penyimpangan semacam itu **gagal keras**.

Jangan pulihkan pola lama itu.

## Sebelum menulis kode

0. Baca [docs/build-order.md](docs/build-order.md) — tangga lapisan L0–L8, gerbang kelulusan, dan **empat aturan mengikat** di bawah. Ia menentukan lapisan mana yang boleh Anda sentuh. Jangan menyimpulkan status dari `docs/checklist.md`: ia melacak kelengkapan dokumentasi, dan `[x]` di sana berarti **tertulis**, bukan **terbangun**.
1. Baca [docs/data/database-design.md](docs/data/database-design.md) **§1 Invarian** dan **§2 Matriks koneksi**.
2. Bila perubahan Anda menyentuh salah satu hand-off di §2, periksa penegaknya masih berlaku.
3. Bila menambah FK, periksa **§5 matriks referential action** — lihat invarian I2.
4. Bila menambah atau mengubah endpoint, perbarui [docs/contracts/api-reference.md](docs/contracts/api-reference.md) pada commit yang sama. Dokumen itu adalah satu-satunya yang boleh dipercaya frontend, dan ia hanya berguna selama ia diturunkan dari kode.

## Empat aturan kerja (dilanggar = pekerjaan dibuang)

Ditetapkan 2026-09-15 setelah audit menemukan tiga milestone dinyatakan selesai
padahal menyimpang dari kontraknya. Rinciannya di [docs/build-order.md](docs/build-order.md) §1.

1. **Definisi selesai adalah skenario acceptance, bukan test hijau.** Docs memuat
   86 skenario acceptance. Test hijau membuktikan kode berjalan; ia tidak pernah
   membuktikan kode benar menurut yang disepakati. Sebut ID skenario yang Anda
   buktikan.
2. **Lapisan tidak boleh berstatus melampaui prasyaratnya.** Jangan membangun L7
   di atas L1 yang belum lulus — bentuk yang salah akan terkunci.
3. **Dilarang mengubah docs kontrak pada commit yang sama dengan kode.** Bila
   kode tidak dapat memenuhi docs, **berhenti dan tanya**. Menyunting pasalnya
   agar cocok dengan kode adalah pembalikan arah. Pengecualian:
   `contracts/api-reference.md` dan `build-order.md` justru wajib ikut.
4. **Docs diam soal mekanisme bukan izin mengarang konsep.** Token management,
   pembuatan vector dan sejenisnya tidak ditulis karena sudah benar bawaannya —
   ikuti yang terbukti. Tetapi kosakata blok, titik promosi memori, urutan
   lifecycle, dan arah D1 adalah **konsep**: mengikat, tanpa tafsir.

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
cargo test --workspace
psql -v ON_ERROR_STOP=1 -d "$APP_DATABASE_URL" -f tests/schema_smoke.sql
./scripts/docs-check.sh              # link mati + endpoint tak terdokumentasi
cargo run -p app -- catalog          # katalog: validasi + prepare SQL ke Fineract
./scripts/integration-test.sh        # permukaan HTTP lewat Bruno CLI
```

`scripts/docs-check.sh` menangkap dua pembusukan dokumen yang paling mudah
luput: link antar-dokumen yang menunjuk file tidak ada, dan endpoint yang ada di
kode tetapi tidak di `docs/contracts/api-reference.md`. Ia tidak memeriksa
kebenaran prosa — itu tetap tanggung jawab yang mengubahnya.

`tests/schema_smoke.sql` menguji **perilaku**, bukan sekadar DDL berhasil di-parse: ia gagal bila K4, fencing, constraint satu-job-per-session, atau rantai CASCADE rusak. Jalankan setelah setiap perubahan migrasi.

## Integration test: Bruno CLI, bukan test integrasi di dalam Rust

**Keputusan yang mengikat.** Permukaan HTTP diuji sebagai HTTP, lewat
[Bruno CLI](https://docs.usebruno.com) terhadap aplikasi yang benar-benar
berjalan dan PostgreSQL yang benar-benar dimigrasi. **Jangan** menambahkan
`tests/*.rs` yang menyalakan server, memakai `axum::Router::oneshot`, atau
sejenisnya.

`cargo test` tetap dipakai — hanya untuk logika murni yang tidak menyentuh
jaringan maupun database (validasi config, verifikasi token, hashing, binding
parameter, komposisi blok response).

```bash
npm install -g @usebruno/cli            # sekali saja; binernya bernama `bru`
./scripts/integration-test.sh           # dua tahap, lihat di bawah
./scripts/integration-test.sh auth      # satu folder saja
./scripts/integration-test.sh engine    # tahap engine saja
PORT=3210 ./scripts/integration-test.sh # port lain bila 3107 dipakai
KEEP_RUNNING=1 ./scripts/integration-test.sh  # app dibiarkan hidup untuk debug
```

Koleksi ada di `fineract-assistant-api/` dengan format **OpenCollection 1.0**
(`opencollection.yml` di root koleksi). Satu request = satu berkas `.yml`:

```yaml
info:
  name: read job
  type: http          # WAJIB; tanpa ini berkas dilewati sebagai "invalid item"
  seq: 3              # urutan di dalam folder
http:
  method: get
  url: "{{baseUrl}}/chat/jobs/{{jobId}}"
  headers:
    - name: authorization
      value: "Bearer {{token}}"
  body:               # hanya untuk POST/PUT
    type: json
    data: |
      { "session_id": "{{sessionId}}" }
runtime:
  scripts:
    - type: before-request
      code: |
        bru.setVar("idemKey", "bruno-" + Date.now());
    - type: tests
      code: |
        test("nama test", function () {
          expect(res.getStatus()).to.equal(200);
        });
        bru.setVar("jobId", res.getBody().data.job_id);
```

Folder punya `folder.yml` (`info.name`, `info.seq`), environment ada di
`environments/local.yml`.

Aturan yang tidak boleh dilanggar saat menulis koleksi:

- **Request dalam satu folder berurutan dan saling bergantung.** Rotasi refresh
  token hanya dapat diuji setelah login; deteksi pemakaian ulang hanya setelah
  rotasi. Jangan berharap satu request lolos bila dijalankan sendirian.
- **Runner berjalan dua tahap.** `health`/`auth`/`chat` dijalankan dengan
  `WORKER_ENABLED=false` karena folder `chat` menguji semantik **penerimaan**
  (job tetap `Queued`, satu job nonterminal per session). Dengan worker menyala,
  job selesai dalam milidetik dan hasil test bergantung pada balapan, bukan pada
  perilaku yang diuji. `engine`/`clarification`/`resolver`/`sse` dijalankan dengan worker menyala
  dan `--delay 1500`.
- **`--disable-cookies` wajib.** Cookie jar otomatis akan menimpa refresh token
  lama, sehingga uji reuse-detection tidak pernah benar-benar berjalan.
- **Kunci `Idempotency-Key` dibuat unik per run** lewat script `before-request`.
  Kunci tetap akan di-replay pada run kedua dan menutupi kegagalan pembuatan job
  yang sebenarnya.
- Test menegaskan **perilaku yang dijanjikan kontrak**, bukan sekadar status
  code: pesan 401 untuk password salah dan user tak dikenal dibandingkan
  byte-per-byte, cursor event diperiksa angkanya, kolom PII yang ditahan
  diperiksa keberadaannya pada blok `limitation`.

Menjalankan `bru` langsung juga sah untuk iterasi cepat, asalkan app sudah
hidup:

```bash
cd fineract-assistant-api
bru run engine -r --env local --env-var baseUrl=http://127.0.0.1:3107 \
  --disable-cookies --delay 1500 --bail
```

## Yang mudah salah

- `knowledge/` dan `queries/` **dibawa apa adanya dan belum direview** — baca `CARRY-OVER.md` di masing-masing folder. Memuat YAML dan `PREPARE` SQL membuktikan sintaks, **bukan** kebenaran.
- Angka di `docs/operations/runtime.md` adalah **nilai awal**, bukan hasil tuning. Setiap angka punya pemicu revisi terukur; jangan ubah tanpa memenuhi pemicunya.
- Beberapa parameter saling terikat dan **tidak boleh diubah sendiri-sendiri** — lihat §10 pemeriksaan konsistensi di dokumen itu, terutama K1, K11 dan K12.
- Jangan menyalin `migrations/` atau kode dari repo lama. Schema-nya berbeda fundamental.
