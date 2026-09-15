# Urutan pengerjaan dan gerbang kelulusan

**Status: kontrak kerja. Mengikat untuk manusia maupun agen.**
Menggantikan `status.md`, yang dihapus 2026-09-15.

---

## Kenapa `status.md` dihapus

`status.md` menjawab "apa yang **berjalan**". Pertanyaan itu terlihat berguna dan
ternyata berbahaya: satu-satunya sinyal yang tersedia untuk menjawabnya adalah
**test hijau**, sehingga "berjalan" pelan-pelan dibaca sebagai "sesuai docs".

Yang terjadi pada 2026-09-15: tiga milestone dinyatakan ✅ dengan 111 unit test
dan 173 test integrasi hijau. Audit terhadap docs menemukan ketiganya menyimpang
dari kontraknya sendiri ([§6](#6-penyimpangan-yang-sudah-ditemukan)). Test hijau
membuktikan kode berjalan; ia tidak pernah membuktikan kode **benar menurut yang
disepakati**.

> **Aplikasi jalan dan aplikasi sesuai adalah dua hal berbeda.**
> Repo ini adalah rewrite justru karena keduanya pernah tertukar.

Dokumen ini menggantinya dengan pertanyaan yang benar: **lapisan mana yang sudah
lulus gerbangnya, dan apa yang boleh dikerjakan berikutnya.**

---

## 1. Empat aturan yang mengikat agen

### Aturan 1 — Definisi selesai adalah skenario acceptance, bukan test hijau

Docs sudah menulis definisi selesai untuk tiap lapisan. Ada **86 skenario
acceptance** yang tersebar di delapan dokumen:

| Dokumen | Skenario |
| --- | --- |
| [contracts/clarifications.md](contracts/clarifications.md) | 15 |
| [contracts/responses.md](contracts/responses.md) | 14 |
| [data/analytical-contracts.md](data/analytical-contracts.md) | 12 |
| [contracts/api.md](contracts/api.md) | 11 |
| [contracts/sse.md](contracts/sse.md) | 10 |
| [data/dataset-lifecycle.md](data/dataset-lifecycle.md) | 10 |
| [architecture/overview.md](architecture/overview.md) | 7 |
| [architecture/memory-context.md](architecture/memory-context.md) | 7 |

Pada 2026-09-15, jumlah test yang merujuk salah satu dari 86 skenario itu:
**nol**. Itu akar masalahnya — bukan docs yang kurang lengkap.

**Aturan**: setiap test menyebut ID skenario yang dibuktikannya. Sebuah baris
status tidak boleh ✅ bila skenarionya tidak punya test.

### Aturan 2 — Lapisan tidak boleh berstatus melampaui prasyaratnya

L7 tidak dapat ✅ selama L1 masih ⬜. Ini mekanis, bukan penilaian.

Alasannya konkret: promosi `session_memory` menulis baris dengan benar, tetapi
fakta yang dipromosikan berasal dari jawaban yang kebenarannya belum pernah
diperiksa (L1/L4), merujuk `dataset_id` yang belum mungkin ada (L3), dan
`handle_state` yang belum menjadi kolom. Menyebutnya "selesai" menyembunyikan
semua itu.

### Aturan 3 — Dilarang mengubah docs kontrak pada commit yang sama dengan kode

Kalau kode tidak dapat memenuhi docs, agen **berhenti dan bertanya**. Menyunting
pasalnya agar cocok dengan kode adalah pembalikan arah: docs adalah riset yang
disepakati sebelum ngoding; kode menyesuaikan docs.

Perubahan kontrak adalah keputusan pemilik repo, dalam commit `docs:` tersendiri,
dengan alasannya.

**Pengecualian** — dua dokumen ini justru **wajib** ikut di commit kode, karena
keduanya mencatat "apa yang ada", bukan "apa yang disepakati":

- [contracts/api-reference.md](contracts/api-reference.md)
- dokumen ini (§5 tabel status)

### Aturan 4 — Docs diam soal mekanisme bukan izin mengarang konsep

Docs sengaja tidak menjelaskan token management, pembuatan vector, dan hal
sejenis: bagian itu sudah benar secara bawaan dan tidak perlu ditulis ulang.

- Docs diam soal **mekanisme** → ikuti yang sudah terbukti, jangan membongkar.
- Docs bicara soal **konsep** → mengikat, tanpa tafsir.

Kosakata blok, titik promosi memori, urutan lifecycle, arah D1 — semuanya
konsep. Melanggarnya sambil mengira sedang memutuskan mekanisme adalah persis
kesalahan yang menghancurkan aplikasi sebelumnya.

---

## 2. Kosakata status

| | Arti |
| --- | --- |
| ⬜ | Belum dibangun. Desainnya mungkin sudah tertulis; itu bukan hal yang sama. |
| 🔨 | Dibangun, **belum** diuji terhadap skenario acceptance. |
| 🧪 | Skenario acceptance lulus, tetapi **prasyaratnya belum** ✅. Batas tertinggi selama prasyarat belum selesai. |
| ✅ | Skenario lulus **dan** seluruh prasyarat ✅. |
| ❌ | Dibangun dan **terbukti menyimpang** dari kontraknya. Wajib diperbaiki, bukan ditambah. |

Tidak ada "sebagian". Kalau perlu kualifikasi, tulis di kolom catatan.

---

## 3. Tangga L0–L8

Urutan ini adalah **dependensi**, bukan prioritas produk. Angkanya tidak dapat
ditukar tanpa mengubah dokumen ini lebih dulu.

### L0 — Fondasi, schema, auth, transport

**Tujuan**: migrasi, invarian I1–I8, envelope, auth, SSE, lease/fencing.
**Dokumen**: [data/database-design.md](data/database-design.md) §1 §2 §5 ·
[architecture/engine.md](architecture/engine.md) · [contracts/sse.md](contracts/sse.md)
**Prasyarat**: —
**Selesai bila**: `tests/schema_smoke.sql` lulus; skenario SSE (10) dan API (11)
punya test ber-ID.
**Status**: 🔨 — mekanismenya berjalan dan teruji, tetapi belum satu pun skenario
dipetakan ke test.

### L1 — Katalog yang benar

**Tujuan**: `knowledge/` + `queries/` berhenti menjadi carry-over yang belum direview.
**Dokumen**: [knowledge/CARRY-OVER.md](../knowledge/CARRY-OVER.md) ·
[queries/CARRY-OVER.md](../queries/CARRY-OVER.md)
**Prasyarat**: L0
**Selesai bila** empat aturan `CARRY-OVER.md` terpenuhi per entri:

1. Contohnya dijalankan ujung ke ujung dan **angkanya dicocokkan dengan SQL
   langsung** ke Fineract lokal (8 office, 43 klien, 15.607 transaksi).
2. Prosa judul/deskripsi konsisten dengan SQL yang dirujuk.
3. Kelas sensitivitas kolom output sesuai kebijakan PII.
4. Semantik cutoff/as-of dideklarasikan.

Ditambah yang belum diperiksa siapa pun menurut `queries/CARRY-OVER.md`: dua
kelas timeout, grain/anti-fanout, kecocokan kolom dengan aturan validasi
response, dan semantik mata uang.

**Status**: ⬜ — 0 dari 48 capability lulus.
**Catatan**: `CARRY-OVER.md` menyatakan sendiri bahwa entri yang belum melewati
empat hal itu **tidak boleh dipakai menjawab pertanyaan yang dipercaya**. Semua
jawaban hari ini berasal dari entri yang belum lulus.

### L2 — Retrieval menemukan capability yang ada

**Tujuan**: pertanyaan pengguna menemukan capability yang benar-benar dimiliki.
**Dokumen**: [architecture/tech-stack.md](architecture/tech-stack.md) ·
[migration/carry-over.md](migration/carry-over.md) #7
**Prasyarat**: L1
**Selesai bila**: pertanyaan berbahasa Indonesia yang capability-nya ada
menemukannya; kegagalan menemukan dibedakan dari di luar cakupan (lihat §6.5).

**Status**: ⬜
**Catatan**: seluruh prosa katalog berbahasa Inggris sementara produknya untuk
pengguna Indonesia. `"berapa total portfolio aktif bulan ini"` menghasilkan **0
match leksikal** walaupun kata `portfolio` ada di 4 entri. Arm embedding belum
ada — kolom `knowledge_index.embedding vector(1024)` terisi 0 dari 192 baris.
Pembuatan vector sendiri tidak diatur docs karena sudah benar bawaannya
(Aturan 4).

### L3 — Dataset handle + chunk

**Tujuan**: hasil besar punya jalur; pagination hasil; handle yang dapat
dinyatakan kedaluwarsa.
**Dokumen**: [data/dataset-lifecycle.md](data/dataset-lifecycle.md) (10 skenario) ·
[migration/carry-over.md](migration/carry-over.md) #11
**Prasyarat**: L0
**Selesai bila**: 10 skenario §8 lulus, termasuk `truncated` ≠ `completeness` ≠
preview, pagination stabil lewat `sort_key_json`, otorisasi dicek ulang tiap
baca (I7), dan dataset `purged` tetap terbaca statusnya.
**Status**: ⬜ — tabel `datasets`/`dataset_chunks` ada sejak migrasi, **0 baris
kode menyentuhnya**. `handle_state` (C13) belum menjadi kolom. Hasil query
ditimbun inline di `job_node_runs.output_json`.

### L4 — Job menjawab dengan benar

**Tujuan**: angka yang keluar terbukti benar, bukan sekadar keluar.
**Dokumen**: [architecture/engine.md](architecture/engine.md) ·
[architecture/overview.md](architecture/overview.md) §6
**Prasyarat**: L1, L3
**Selesai bila**: untuk tiap capability yang dipakai, hasil job dibandingkan
dengan SQL langsung ke Fineract dan cocok.
**Status**: 🔨 — mesin job (lifecycle 8 state, fencing, idempotency, reaper)
berjalan dan teruji. **Kebenaran jawaban tidak pernah diuji siapa pun.**

### L5 — Response document sesuai kontrak

**Tujuan**: bentuk dokumen sesuai [contracts/responses.md](contracts/responses.md) §1–§2.
**Prasyarat**: L0 (dapat dikerjakan paralel dengan L1–L4; lihat §4)
**Selesai bila**: tiap blok membawa `block_id`, `type`, `schema_version`,
`derived_from`; kosakata terbatas pada 9 tipe §2; lineage hidup di
`evidence_json`, bukan sebagai blok.
**Status**: ❌ — menyimpang, lihat §6.2.

### L6 — Validator D1–D3 penuh

**Tujuan**: penegakan runtime yang benar-benar menegakkan kontrak.
**Dokumen**: [contracts/responses.md](contracts/responses.md) §3–§6 (14 skenario)
**Prasyarat**: L5
**Selesai bila**: D1 dihitung **per blok** lewat `derived_from` lalu diagregasi
(§3 aturan 2–3); D2 memeriksa blok `note` (§5); D3 memakai daftar pengecualian
tertutup §4; §1 dan §2 ditegakkan — tipe di luar kosakata ditolak.
**Status**: ❌ — aproksimasi, lihat §6.3.

### L7 — Klarifikasi lengkap dan memory yang berguna

**Tujuan**: percakapan, bukan pertanyaan berturut-turut.
**Dokumen**: [contracts/clarifications.md](contracts/clarifications.md) (15 skenario) ·
[architecture/memory-context.md](architecture/memory-context.md) (7 skenario)
**Prasyarat**: L1, L2, L3, L4, L5, L6
**Selesai bila**: 22 skenario lulus, termasuk promosi pada T8 skip (§7-3),
`handle_state` non-optional (§5, C13), seleksi konteks berbudget (§5), dan
ringkasan inkremental berwatermark (§4).
**Status**: ❌/🔨 — jalur tulis memori benar dan terbukti, tetapi T8 skip
menyimpang (§6.1), tidak ada pembacanya, ringkasan tidak pernah dihitung, dan
`dataset_id` selalu NULL karena L3 belum ada.

### L8 — Lapisan di atasnya

LLM additive, plan multi-node + fan-in, analytical contract Mode 2, security
final, observability, OpenAPI.
**Prasyarat**: L7.
**Status**: ⬜ — **jangan disentuh** sebelum L1–L7 ✅.

---

## 4. Apa yang boleh dikerjakan paralel

Beberapa agen sekaligus **boleh**, asalkan tidak melompati prasyarat.

| Jalur | Isi | Bentrok dengan |
| --- | --- | --- |
| **A — Katalog (L1)** | 48 capability diverifikasi satu per satu | tidak ada; paling mudah diparalelkan, satu agen per domain |
| **B — Dataset (L3)** | `datasets`/`dataset_chunks`, murni lapisan penyimpanan | tidak ada |
| **C — Bentuk dokumen (L5+L6)** | `compose.rs`, `validate.rs`, `evidence_json` | jangan bersamaan dengan jalur lain yang menyentuh `compose.rs` |
| **D — Pemetaan skenario** | beri ID pada 86 skenario, tulis `scripts/acceptance-check.sh` | menyentuh seluruh docs — kerjakan **sendirian**, jangan paralel |

Jalur D sebaiknya **didahulukan dan diselesaikan lebih dulu**: tanpa peta
skenario, jalur A/B/C tidak punya cara membuktikan dirinya selesai.

L2 menunggu L1. L4 menunggu L1 dan L3. L7 menunggu semuanya.

---

## 5. Ringkas status

| Lapisan | Status | Prasyarat lulus? |
| --- | --- | --- |
| L0 Fondasi | 🔨 | — |
| L1 Katalog benar | ⬜ | L0 🔨 |
| L2 Retrieval | ⬜ | L1 ⬜ |
| L3 Dataset | ⬜ | L0 🔨 |
| L4 Job menjawab benar | 🔨 | L1 ⬜, L3 ⬜ |
| L5 Bentuk response | ❌ | L0 🔨 |
| L6 Validator | ❌ | L5 ❌ |
| L7 Klarifikasi + memory | ❌ | enam lapisan di bawahnya belum ✅ |
| L8 Di atasnya | ⬜ | L7 ❌ |

**Tidak ada satu pun lapisan berstatus ✅.** Itu keadaan sebenarnya pada
2026-09-15, dan lebih berguna daripada daftar ✅ yang tidak dapat dipertanggungjawabkan.

---

## 6. Penyimpangan yang sudah ditemukan

Wajib diperbaiki sebelum lapisan di atasnya ditambah. Ditemukan lewat audit
kode terhadap docs, 2026-09-15.

### 6.1 T8 skip tidak mempromosikan memori — melanggar kontrak

[memory-context.md](architecture/memory-context.md) §3: *"Satu-satunya titik
promosi adalah response commit T7, **termasuk T8 skip**"*; §7-3 mewajibkan
"promosi memori atomik"; [migration/carry-over.md](migration/carry-over.md) K2:
*"Context tersimpan ke memory pada commit itu"*; K3 membedakan skip (promosi)
dari cancel (tidak).

`crates/chat/src/clarification/repository.rs` justru menulis *"Tidak ada promosi
memori di sini, dan itu keputusan"*. Keputusan sepihak melawan kontrak.

### 6.2 Bentuk blok menyimpang dari §1 dan §2

| Kontrak | Nyata |
| --- | --- |
| `block_id` wajib | dipakai `id` |
| `schema_version` per blok wajib | tidak ada |
| `derived_from` wajib pada blok data | tidak ada |
| Kosakata 9 tipe §2 | dipancarkan `provenance` (95 blok) dan `metrics` (65 blok); keduanya **tidak ada di kosakata**. `metric` seharusnya tunggal |
| Auto-bind diungkap pada blok `note` (§5) | diungkap pada blok `limitation`; `note` **tidak pernah dipakai** |
| Lineage di `evidence_json` (#10) | `evidence_json` selalu `{}`; lineage dikarang jadi blok `provenance` |

### 6.3 Validator menegakkan implementasi, bukan kontrak

- D1 dihitung di tingkat dokumen, bukan per blok — karena `derived_from` tidak
  ada. Aproksimasi ini tidak pernah dinyatakan.
- D2 memindai blok mana pun yang punya `auto_bound_slots`, bukan blok `note`,
  sehingga ia **lulus terhadap bentuk yang salah**.
- §1 dan §2 tidak ditegakkan sama sekali.

### 6.4 `Cancelled`/`Expired` bermakna `OperationalFailure`

32 job terminal tercatat `outcome='OperationalFailure'` padahal dibatalkan atau
kedaluwarsa. K3 menyatakan cancel adalah abort, bukan kegagalan operasional.
Dashboard yang mewarnai berdasar `outcome` akan menampilkan error palsu.

### 6.5 `Unsupported` mencampur empat sebab yang berbeda

| `completeness_reason` | Jumlah | Artinya |
| --- | --- | --- |
| `no_capability_matched` | 37 | **campur** — sebagian benar di luar cakupan, sebagian kegagalan retrieval kita |
| `identity_slot_without_resolver` | 11 | katalog belum lengkap |
| `planner_not_implemented` | 6 | fitur belum ada |
| `parameter_binding_unsupported` | 2 | fitur belum ada |

"Di luar cakupan" dan "kami gagal menemukan capability yang kami punya" terlihat
identik di data dan di UI. Keduanya wajib dipisah.

---

## 7. Pemeriksaan yang wajib hijau

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p app -- catalog
psql -v ON_ERROR_STOP=1 -d "$APP_DATABASE_URL" -f tests/schema_smoke.sql   # bila migrasi berubah
./scripts/docs-check.sh
PORT=3107 ./scripts/integration-test.sh
```

Semuanya hijau **tidak** berarti sesuai docs. Ia hanya prasyarat minimum sebelum
pertanyaan kesesuaian boleh diajukan.

Catatan lingkungan: port 3007 dan Redis 6380 dipakai repo lama pada mesin
pengembangan ini — jalankan dengan `APP_PORT=3107`. `WORKER_ENABLED` default
`true`; instance yang tertinggal di port mana pun akan mengklaim job dari tahap
intake dan merusak assertion "job tetap `Queued`" — hentikan dulu, bukan pindah
port.

---

## 8. Menjalankan lokal

```bash
cd fineract-ai-backend
docker compose up -d
sqlx migrate run --database-url "$APP_DATABASE_URL"
APP_PORT=3107 cargo run -p app
```

Frontend: mulai dari [contracts/api-reference.md](contracts/api-reference.md) —
permukaan yang benar-benar ada, payload disalin dari aplikasi berjalan. Ingat
bahwa "ada" tidak berarti "sesuai": lihat §6.2 sebelum mengunci bentuk render.
