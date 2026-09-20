# Referensi API — permukaan yang benar-benar ada

Status: **diturunkan dari kode, bukan dari rencana.** Setiap payload di dokumen
ini disalin dari aplikasi yang berjalan terhadap Fineract lokal (8 office,
43 klien, 15.607 transaksi) pada commit yang sama dengan dokumen ini. Endpoint
yang belum dibangun **tidak** dicantumkan di sini — lihat
[status implementasi](../build-order.md) untuk apa yang belum ada.

Bedanya dengan [api.md](api.md): `api.md` adalah kontrak yang disepakati
(termasuk yang belum dibangun); dokumen ini adalah permukaan yang dapat
di-integrasikan frontend **hari ini**.

---

## 1. Aturan yang berlaku di seluruh permukaan

### Envelope

Setiap response HTTP JSON memakai bentuk yang sama, termasuk error:

```json
{ "success": true,  "data": { }, "error": null }
{ "success": false, "data": null, "error": { "code": "CONFLICT", "message": "..." } }
```

`data` dan `error` tidak pernah terisi bersamaan. Frontend boleh mengandalkan
ini tanpa memeriksa status code lebih dulu.

### Matriks error

| Status | `error.code` | Arti | Tindakan klien |
| --- | --- | --- | --- |
| 401 | `UNAUTHORIZED` | Bearer tidak ada, kedaluwarsa, atau tidak sah | Refresh token, lalu ulangi |
| 403 | `FORBIDDEN` | Terautentikasi, tetapi tidak berizin | Jangan ulangi |
| 404 | `NOT_FOUND` | Tidak ada, **atau** milik pengguna lain | Jangan ulangi. Job milik orang lain sengaja tampak identik dengan job yang tidak ada |
| 409 | `CONFLICT` | Konflik state; `message` menyebut sebabnya | Baca `message`; biasanya butuh snapshot baru |
| 422 | `INVALID` | Validasi gagal; `message` dapat berupa JSON array field error | Perbaiki input |
| 500 | `INTERNAL` | Kegagalan tak terduga | Pesan selalu generik — SQL, prompt dan stack tidak pernah bocor |

Pada `422` dari `POST /chat/jobs/{id}/responses`, `error.message` adalah **string
berisi JSON** yang harus di-parse sekali lagi:

```json
{"success":false,"data":null,"error":{"code":"INVALID","message":"[{\"field_id\":\"client_id\",\"code\":\"option_not_issued\",\"message\":\"This option was never issued for this clarification\"}]"}}
```

Kode field yang mungkin: `unknown_field`, `required`, `too_long`,
`type_mismatch`, `option_not_issued`, `option_out_of_scope`,
`binding_type_mismatch`.

### Autentikasi

`Authorization: Bearer <access_token>` pada semua endpoint kecuali
`/health`, `/auth/login` dan `/auth/refresh`. **Termasuk SSE** — token tidak
pernah boleh dipindahkan ke query string.

### Idempotency

Setiap operasi tulis job (`POST /chat/jobs`, `POST /chat/jobs/{id}/responses`)
wajib membawa header `Idempotency-Key`, 16–255 karakter. Kunci di luar rentang
itu menghasilkan `422`, bukan `500`.

Untuk `POST /chat/jobs`, kunci yang sama dengan payload yang sama mengembalikan
acknowledgement tersimpan apa adanya (job_id yang **sama**, tanpa job kedua).
Kunci sama dengan payload berbeda menghasilkan `409`.

---

## 2. Health

### `GET /health`

Tanpa auth.

```json
{
  "success": true,
  "data": {
    "status": "ok",
    "app_database": "ok",
    "fineract_database": "ok",
    "redis": "live"
  },
  "error": null
}
```

`redis` bernilai `live`, `unavailable`, atau `disabled`. Redis mati **bukan**
kegagalan sistem: ia hanya koordinasi live.

---

## 3. Autentikasi

### `POST /auth/login`

```json
{ "username": "admin", "password": "..." }
```

```json
{
  "success": true,
  "data": {
    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...",
    "token_type": "Bearer",
    "expires_in": 900,
    "user": {
      "id": "703a14e9-fcbf-4a6a-9c93-0fe92e17bbac",
      "username": "admin",
      "email": "admin@example.com",
      "full_name": null,
      "role": "admin"
    }
  },
  "error": null
}
```

Refresh token **tidak** ada di body: ia dikirim sebagai cookie `HttpOnly`
(`refresh_token`). Frontend tidak pernah menyentuhnya.

Pesan `401` untuk password salah dan untuk user tidak dikenal **identik
byte-per-byte** — itu disengaja, dan diuji.

### `POST /auth/refresh`

Tanpa body. Memakai cookie refresh token.

Rotasi: setiap refresh menerbitkan refresh token baru dan mematikan yang lama.
Memakai refresh token lama setelah rotasi terdeteksi sebagai **reuse**: seluruh
rantai token pada auth session itu dimatikan, dan refresh berikutnya — termasuk
dengan token terbaru — menghasilkan `401`. Frontend harus memperlakukannya
sebagai "login ulang", bukan sebagai kesalahan sementara.

### `POST /auth/logout`

Mematikan auth session dan menghapus cookie.

### `GET /auth/me`

```json
{
  "success": true,
  "data": {
    "id": "703a14e9-fcbf-4a6a-9c93-0fe92e17bbac",
    "username": "admin",
    "email": "admin@example.com",
    "full_name": null,
    "role": "admin"
  },
  "error": null
}
```

---

## 4. Session

### `POST /chat/sessions` → `201`

```json
{ "title": "..." }
```

```json
{
  "success": true,
  "data": {
    "id": "bcd23794-0435-486d-84cb-09809f146ccd",
    "owner_user_id": "703a14e9-fcbf-4a6a-9c93-0fe92e17bbac",
    "title": "capture",
    "status": "active",
    "created_at": "2026-09-15T04:39:01.377025Z",
    "updated_at": "2026-09-15T04:39:01.377025Z"
  },
  "error": null
}
```

### `GET /chat/sessions`

Keyset pagination. Query opsional: `before_updated_at`, `before_id`, `limit`.

```json
{
  "success": true,
  "data": {
    "sessions": [ { "id": "...", "title": "capture", "status": "active", "created_at": "...", "updated_at": "..." } ],
    "next_before_updated_at": "2026-09-15T03:30:40.536333Z",
    "next_before_id": "71f9529f-c8b0-4571-ab9c-92f89167ce66"
  },
  "error": null
}
```

Halaman berikutnya: kirim kembali kedua `next_*` sebagai `before_*`. Keduanya
`null` berarti habis.

### `GET /chat/sessions/{session_id}`

Bentuk sama dengan hasil `POST`.

### `GET /chat/sessions/{session_id}/messages`

Riwayat percakapan untuk render ulang sesudah refresh. Keyset pagination,
`created_at DESC, id DESC` — **terbaru lebih dulu**. Query opsional:
`before_created_at`, `before_id` (wajib berpasangan; satu saja → `422`),
`limit` (maksimum 50). Session milik orang lain atau tidak dikenal → `404`.

```json
{
  "success": true,
  "data": {
    "messages": [
      {
        "id": "0c6f2a1e-6f4e-4c38-9f03-6a0b6a7c9a11",
        "job_id": "f51f9e7a-2a4d-4a55-9b9c-0c2a9f0c4c31",
        "role": "assistant",
        "request_text": null,
        "response_version": 1,
        "clarification_id": null,
        "clarification_revision": null,
        "created_at": "2026-09-15T04:39:21.767871Z"
      },
      {
        "id": "b2b3f0dd-1f77-4a3e-9d18-2d4fd0b7f0a2",
        "job_id": "f51f9e7a-2a4d-4a55-9b9c-0c2a9f0c4c31",
        "role": "user",
        "request_text": "berapa total saldo tabungan?",
        "response_version": null,
        "clarification_id": null,
        "clarification_revision": null,
        "created_at": "2026-09-15T04:39:19.204118Z"
      }
    ],
    "next_before_created_at": "2026-09-15T04:39:19.204118Z",
    "next_before_id": "b2b3f0dd-1f77-4a3e-9d18-2d4fd0b7f0a2"
  },
  "error": null
}
```

**Indeksnya tipis, dan itu disengaja.** Teks tidak disalin ke riwayat; ia tetap
tinggal di rumahnya yang immutable. Karena itu:

- `role`: `user`, `assistant`, atau `clarification`.
- `request_text` hanya terisi pada baris `user` — pada baris lain nilainya
  `null`, bukan pertanyaan yang sama diulang.
- Isi jawaban diambil lewat `GET /chat/jobs/{job_id}/response`
  (`response_version` menyebut versi yang ditulis turn itu).
- Isi form klarifikasi diambil lewat `GET /chat/jobs/{job_id}/clarification`
  (`clarification_id` + `clarification_revision` menyebut revisi turn itu).

---

## 5. Job

### `POST /chat/jobs` → `202`

Header wajib: `Idempotency-Key`.

```json
{
  "session_id": "8486bf19-802c-45da-9366-b65bcf2311bc",
  "request_text": "Show the savings portfolio summary.",
  "office_ids": []
}
```

`office_ids` opsional. Ia hanya **mempersempit** scope; ia tidak pernah
memperluas izin. Kosong berarti seluruh office yang diizinkan.

```json
{
  "success": true,
  "data": {
    "job_id": "f2f939e2-e45f-4a5c-a019-57d325eb11f3",
    "session_id": "8486bf19-802c-45da-9366-b65bcf2311bc",
    "lifecycle": "Queued",
    "event_cursor": 1
  },
  "error": null
}
```

**`202` berarti penerimaan yang durable, bukan analisis yang berhasil.**
`event_cursor` adalah titik mulai berlangganan SSE.

`409` yang mungkin:

| `message` | Sebab |
| --- | --- |
| `Another job is still active in this session` | Satu job nonterminal per session |
| `Idempotency-Key was already used with a different payload` | Kunci dipakai ulang |
| `An identical request is still being processed` | Permintaan pertama belum selesai |
| `Session is <status>, not active` | Session tidak aktif |

### `GET /chat/jobs/{job_id}`

Snapshot konsisten. Inilah sumber `event_cursor` untuk reconnect SSE.

```json
{
  "success": true,
  "data": {
    "id": "f2f939e2-e45f-4a5c-a019-57d325eb11f3",
    "session_id": "8486bf19-802c-45da-9366-b65bcf2311bc",
    "owner_user_id": "703a14e9-fcbf-4a6a-9c93-0fe92e17bbac",
    "request_text": "Show the savings portfolio summary.",
    "lifecycle": "Completed",
    "outcome": "Answered",
    "completeness": "Complete",
    "completeness_reason": "curated_query:savings.balance_summary",
    "failure_code": null,
    "plan_version": 1,
    "final_response_version": 1,
    "last_event_sequence": 5,
    "scope_json": {
      "fineract_tenant": "default",
      "office_ids": [],
      "pii": { "enabled": false, "mode": "withhold", "setting_version": 1 },
      "source": "admin_projection"
    },
    "created_at": "2026-09-15T04:39:21.248060Z",
    "terminal_at": "2026-09-15T04:39:21.767871Z"
  },
  "error": null
}
```

**Tiga dimensi status terpisah, dan frontend tidak boleh menggabungkannya:**

| Field | Nilai | Catatan |
| --- | --- | --- |
| `lifecycle` | `Queued`, `Running`, `WaitingForUser`, `Cancelling`, `Completed`, `Failed`, `Cancelled`, `Expired` | `WaitingForUser` adalah **suspensi**, bukan terminal |
| `outcome` | `Answered`, `Empty`, `NotFound`, `Unsupported`, `BlockedByPolicy`, `Invalid`, `OperationalFailure`, `SkippedByUser` | `null` selama lifecycle nonterminal |
| `completeness` | `Complete`, `Partial`, `Unknown` | `null` selama lifecycle nonterminal |

`Completed` berarti ada response document yang durable — **bukan** bahwa
jawabannya lengkap atau ditemukan. `Completed` + `Unsupported` + `Unknown`
adalah kombinasi yang sah dan sering.

Kombinasi yang dilarang (dan tidak akan pernah dikirim): `Empty`/`NotFound`
dengan `Partial`; `SkippedByUser` dengan `Complete`; lifecycle nonterminal
dengan `outcome` terisi.

### `GET /chat/jobs/{job_id}/response`

`404` selama `final_response_version` masih `null`. Frontend **tidak boleh**
menyimpulkan hasil dari `lifecycle` saja.

```json
{
  "success": true,
  "data": {
    "response_version": 1,
    "kind": "analysis",
    "outcome": "Answered",
    "completeness": "Complete",
    "completeness_reason": "curated_query:savings.balance_summary",
    "blocks_json": [ ... ],
    "evidence_json": { "lineage": [ ... ], "derivations": [] },
    "validation_status": "passed",
    "response_hash": "fddab222...",
    "created_at": "2026-09-15T04:39:21.767871Z"
  },
  "error": null
}
```

`kind`: `analysis`, `limitation`, atau `skipped`.

#### `validation_status` dan kenapa `response_version` dapat bernilai 2

Setiap response `analysis` **dihitung ulang terhadap ledger** sebelum di-commit
(responses.md §1–§5): bentuk tiap blok dan kosakatanya ditegakkan,
`completeness` dihitung **per blok** lewat `derived_from` lalu diagregasi
menjadi klaim dokumen dan dibandingkan, himpunan slot auto-bind pada blok `note`
dibandingkan dengan binding yang benar-benar dikonsumsi node, dan setiap angka
pada narasi wajib cocok dengan blok ber-evidence atau entri `derivation`.

| `validation_status` | Arti bagi frontend |
| --- | --- |
| `passed` | Dokumen disajikan apa adanya |
| `fallback` | Versi konservatif. Ada blok yang **dibuang**, dan `blocks_json` memuat blok `limitation` ber-`block_id` `validation_rejected` yang menyebutkan apa dan kenapa |
| `failed` | **Tidak pernah dikembalikan endpoint ini.** Ia versi yang ditolak, disimpan hanya sebagai bahan investigasi |

Karena itu `response_version` dapat bernilai `2`: versi 1 adalah dokumen yang
ditolak, versi 2 adalah fallback yang disajikan. Endpoint selalu mengembalikan
`final_response_version` — frontend tidak perlu, dan tidak boleh, menebak
versinya sendiri.

Bila seluruh blok data terbuang, hasilnya `kind: "limitation"` dengan
`outcome: "Unsupported"` dan `completeness: "Unknown"` — bukan dokumen kosong,
dan bukan job yang gagal diam-diam. Event `job.completed` juga membawa
`validation_status`, jadi klien SSE mengetahuinya tanpa membaca ulang dokumen.

#### Bentuk blok

Setiap blok membawa **empat** field wajib (responses.md §1):

| Field | Wajib | Keterangan |
| --- | --- | --- |
| `block_id` | ya | stabil di dalam satu `response_version`. **Bukan `id`** |
| `type` | ya | salah satu dari sembilan tipe di bawah, tidak pernah di luar itu |
| `schema_version` | ya | **per blok**, bukan per dokumen. Bernilai `1` hari ini |
| `derived_from` | pada blok penyaji data | daftar rujukan `{ "node_run_id": "…" }` dan/atau `{ "dataset_id": "…" }` |

Kosakata blok tertutup pada sembilan tipe: `narrative`, `metric`, `table`,
`chart_spec`, `comparison`, `finding`, `limitation`, `suggestion`, `note`.
Tipe di luar daftar itu **ditolak validator** dan tidak akan pernah dikirim.

Blok yang tidak dikenal wajib diabaikan, bukan menggagalkan render. Karena itu
informasi yang wajib sampai — limitation, pengungkapan auto-bind, PII yang
ditahan — tidak pernah hanya hidup di tipe blok baru.

> **Perubahan bentuk terhadap versi sebelumnya.** Sampai 2026-09-15 blok memakai
> `id`, tanpa `schema_version` dan tanpa `derived_from`, dan memancarkan dua tipe
> yang tidak ada di kontrak: `provenance` dan `metrics` (jamak). Keduanya hilang.
> `metrics` menjadi **`metric` tunggal, satu blok per nilai bernama**;
> `provenance` menjadi kolom `evidence_json`. Pengungkapan auto-bind pindah dari
> `limitation` ke `note`.

#### Blok yang benar-benar dipancarkan hari ini

**`metric`** — satu nilai bernama. Hasil satu baris berisi tiga kolom menjadi
**tiga blok `metric`**, bukan satu blok berisi daftar:

```json
{
  "block_id": "metric:total_balance",
  "type": "metric",
  "schema_version": 1,
  "derived_from": [{ "node_run_id": "9f0c1f9e-6c5a-4a1e-9c1a-0f2b7c3d5e11" }],
  "key": "total_balance",
  "value": "486705.19",
  "unit": null,
  "period": { "as_of_date": "2026-09-15" }
}
```

`unit` bernilai `null` selama katalog belum menyatakannya — `null` berarti
**tidak diketahui**, dan field itu tidak dihilangkan supaya perbedaan itu
terlihat.

> Angka `NUMERIC` dikirim sebagai **string**, bukan float. Pembulatan biner pada
> angka uang adalah cara klasik total berubah satu sen tanpa ada yang
> menyadarinya. Frontend wajib memformatnya sebagai desimal, bukan `Number()`.

**`table`** — hasil banyak baris:

```json
{
  "block_id": "result",
  "type": "table",
  "schema_version": 1,
  "derived_from": [{ "node_run_id": "9f0c1f9e-6c5a-4a1e-9c1a-0f2b7c3d5e11" }],
  "columns": ["savings_product_id", "savings_product_name", "client_id"],
  "rows": [[1, "Current Account - USD", 1], [9, "Current Account With OD - AED", 1]],
  "row_count": 2,
  "withheld_columns": []
}
```

`withheld_columns` dideklarasikan pada tabel **dan** dinyatakan pada blok
`limitation`; keduanya, bukan salah satu.

**`narrative`** — kalimat. Angka di dalamnya wajib ada di blok lain atau
ber-`derivation`; bila ia memuat angka, ia juga wajib membawa `derived_from`.

**`note`** — pengungkapan asumsi/binding. Satu-satunya tempat pengungkapan
auto-bind dihitung sah (responses.md §5):

```json
{
  "block_id": "slots_auto_bound",
  "type": "note",
  "schema_version": 1,
  "title": "Values chosen without asking",
  "body": "1 value(s) were bound automatically …",
  "auto_bound_slots": [
    { "field_id": "client_id", "label": "Siti", "provenance": "resolver_unique" }
  ]
}
```

**`limitation`** — pembatasan yang wajib ditampilkan, tidak boleh disembunyikan
di balik "lihat detail". `block_id` yang ada hari ini:

| `block_id` | Arti | Field tambahan |
| --- | --- | --- |
| `pii_withheld` | Kolom PII ditahan karena sakelar PII mati | `withheld_columns` |
| `skipped_inputs` | Pengguna berhenti; input yang tidak pernah diisi | `unanswered_fields`, `completed_nodes` |
| `validation_rejected` | Ada blok yang dibuang validator | `failed_rules`, `failures` |
| `resolver_no_candidates` | Resolver berjalan utuh dan tidak menemukan kandidat dalam scope | — |
| `no_capability_matched` | Tidak ada capability yang disetujui mencakup permintaan | `request_echo` |
| `identity_slot_without_resolver` | Slot identitas tanpa resolver; tidak dapat ditanyakan (K1) | `request_echo` |
| `parameter_needs_clarification` | Parameter kurang dan tidak dapat diturunkan | `request_echo` |
| `source_query_timeout`, `source_query_failed` | Query sumber tidak selesai; hasilnya **tidak diketahui**, bukan nol | — |

#### `evidence_json` — lineage

Dari mana angkanya berasal. Ia **kolom, bukan blok**: blok yang tidak dikenal
klien boleh dilewati, dan jejak asal angka tidak boleh ikut hilang bersamanya.

```json
{
  "lineage": [
    {
      "node_run_id": "9f0c1f9e-6c5a-4a1e-9c1a-0f2b7c3d5e11",
      "dataset_id": null,
      "capability_id": "savings_balance_summary",
      "query_id": "savings.balance_summary",
      "sql_file": "queries/savings/balance_summary.sql",
      "catalog_version_id": "18552c18-2502-4318-9b6e-3d14f769cb17",
      "catalog_content_hash": "a375d49a...",
      "as_of": "2026-09-15",
      "exchange_rate_id": null,
      "parameters": [
        { "name": "office_ids", "value": "8 authorized offices" },
        { "name": "currency_code", "value": null }
      ],
      "row_count": 1,
      "duration_ms": 31
    }
  ],
  "derivations": []
}
```

Scope dicatat sebagai **jumlah**, bukan daftar office. `derivations` kosong
selama belum ada narasi model; ia adalah satu-satunya jalan angka turunan
("naik 12%") menjadi sah (responses.md §4).

Dokumen `limitation` yang tidak pernah menjalankan operasi sumber membawa
`evidence_json: {}` — tidak ada lineage karena memang tidak ada operasi.

### `POST /chat/jobs/{job_id}/cancel`

Mengembalikan snapshot job sesudah permintaan. Cancel pada job terminal adalah
no-op, bukan error. Cancel berbeda dari skip: ia berakhir `Cancelled`, tanpa
response document.

---

## 5b. Dataset

Hasil besar disimpan sebagai *dataset* immutable, bukan ditumpahkan inline ke
response job. Keduanya hanya baca — dataset dibuat worker dan tidak pernah
di-UPDATE (koreksi = dataset baru). Setiap pembacaan memeriksa ulang otorisasi
terhadap scope yang berlaku **sekarang** (I7); handle adalah rujukan, bukan izin.

`handle_state` **selalu** ada (C13) dengan salah satu nilai: `live`, `expired`,
`purged`, `none` (belum ter-materialisasi / gagal). Handle mati tetap dijawab
`200` beserta `handle_state`-nya dan `unavailable_reason` (`dataset_expired`,
`dataset_purged`, `dataset_not_materialized`) — dataset kedaluwarsa **masih
terbaca statusnya**, bukan 404 dan bukan halaman kosong yang tampak seperti nol
(§7, I5). Dataset milik user lain dijawab `404` (keberadaannya tidak diungkap);
scope yang menyempit dijawab `403`.

`truncated` = set tersimpan dibatasi cap, bukan klaim analitik dan bukan preview
(I4). `row_count_total = null` berarti **tidak diketahui**, bukan nol.

### `GET /chat/datasets/{dataset_id}`

Metadata handle. `200` dengan:

```json
{
  "success": true,
  "data": {
    "dataset_id": "…", "job_id": "…", "session_id": "…",
    "node_id": null, "plan_version": null,
    "handle_state": "live",
    "status": "ready",
    "schema": {}, "grain": {}, "scope": {}, "provenance": {}, "sort_key": {},
    "completeness": "Complete", "completeness_reason": null,
    "truncated": false,
    "row_count_available": 120, "row_count_total": 120, "byte_size": 4096,
    "chunk_count": 1,
    "created_at": "…", "expires_at": null, "purged_at": null,
    "unavailable_reason": null
  },
  "error": null
}
```

### `GET /chat/datasets/{dataset_id}/rows`

Satu halaman baris, keyset stabil atas `sort_key` (§4 — halaman berbeda tidak
mengubah urutan). Query: `cursor` (opsional; kosong = mulai dari baris pertama,
cursor rusak → `422`), `limit` (opsional, di-clamp `1..=200`). `200` dengan:

```json
{
  "success": true,
  "data": {
    "dataset_id": "…",
    "handle_state": "live",
    "sort_key": {},
    "rows": [],
    "cursor": "0:0",
    "next_cursor": null,
    "row_count_available": 120, "row_count_total": 120,
    "truncated": false,
    "completeness": "Complete", "completeness_reason": null,
    "unavailable_reason": null
  },
  "error": null
}
```

`next_cursor: null` menandai halaman terakhir. Handle mati mengembalikan `rows: []`
dengan `unavailable_reason` terisi — bukan halaman kosong tanpa penjelasan.

---

## 6. Klarifikasi

### `GET /chat/jobs/{job_id}/clarification`

`404` bila tidak ada form terbuka.

```json
{
  "success": true,
  "data": {
    "id": "03578260-05b0-43de-b148-5fca376f4810",
    "job_id": "405e0df2-4f1e-417a-aa41-b40a692650c7",
    "clarification_id": "a9e0f0f2-617e-4bd4-b90b-30f0f2f134df",
    "revision": 1,
    "schema_version": 1,
    "purpose": "Missing input for capability 'savings_products_by_client'",
    "stage_label": "Additional input needed",
    "fields_json": [
      {
        "field_id": "client_id",
        "type": "single_choice",
        "parameter_kind": "integer",
        "label": "client id",
        "required": true,
        "resolver": {
          "dataset_id": "client.identity",
          "shape_id": "identity_candidates",
          "output_slot": "client_id"
        },
        "resolver_ref": "client.identity_resolve",
        "candidate_count": 43,
        "candidates_truncated": false,
        "options_path": "/chat/jobs/405e0df2-.../clarification/options?field_id=client_id"
      }
    ],
    "state": "open",
    "expires_at": "2026-09-15T05:13:49.053443Z",
    "created_at": "2026-09-15T03:13:49.053443Z"
  },
  "error": null
}
```

`stage_label` adalah label deskriptif, **bukan** "step 2 of 5": jumlah tahap
tidak pernah dikarang, jadi frontend tidak boleh merendernya sebagai progress
bar bertahap.

`expires_at` adalah batas waktu form ini sendiri (`CLARIFICATION_WAIT_LIMIT`,
default 2 jam), bukan TTL job berjalan.

#### Tipe field

| `type` | Cara menjawab |
| --- | --- |
| `single_choice` | Kirim `option_id` dari endpoint opsi. **Tidak pernah** teks bebas |
| `text` | Teks, maksimal 512 karakter |
| `number` | Bilangan bulat |
| `date` | `YYYY-MM-DD` |
| `boolean` | `true`/`false` |

Field dengan blok `resolver` **selalu** `single_choice`. Bila sebuah slot
identitas tidak punya resolver yang disetujui, ia tidak muncul sebagai field
sama sekali — job dijawab `Unsupported` dengan
`completeness_reason: "identity_slot_without_resolver"`.

### `GET /chat/jobs/{job_id}/clarification/options`

Query: `field_id` (wajib), `cursor` (default 0), `limit`, `q`.

```json
{
  "success": true,
  "data": {
    "clarification_id": "fdf08c7d-1348-48ee-aa98-51fc1ca3b1d5",
    "revision": 1,
    "field_id": "client_id",
    "resolver_ref": "client.identity_resolve",
    "options": [
      {
        "option_id": "1",
        "label": "Client 1",
        "attributes": { "office_id": 2, "office_name": "Branch 001", "client_status_enum": 300 }
      }
    ],
    "cursor": 0,
    "next_cursor": 25,
    "matched_total": 43,
    "truncated": false
  },
  "error": null
}
```

- `next_cursor` `null` berarti halaman terakhir.
- `matched_total` adalah jumlah kandidat **dalam scope**, bukan jumlah yang
  dikirim.
- `truncated: true` berarti kandidat melewati `RESOLVER_MAX_CANDIDATES`; daftar
  ini bukan populasi lengkap dan klien wajib mempersempit dengan `q`.
- `q` menyaring atas kandidat yang **sudah** ter-scope. Ia tidak pernah
  memperluas scope.
- `label` tunduk pada sakelar PII. Saat PII mati, label memakai fallback katalog
  (`Client 1`), bukan nama nasabah. Ini fail-closed, bukan bug.
- Urutan opsi stabil dan numerik, sehingga halaman berikutnya tidak tumpang
  tindih.

**Hanya halaman yang benar-benar dikirim yang dapat dijawab.** `option_id` yang
tidak pernah diterbitkan untuk form+field ini ditolak `422`
`option_not_issued` — termasuk id yang valid di job lain.

### `POST /chat/jobs/{job_id}/responses`

Header wajib: `Idempotency-Key`.

**Menjawab** → `202`:

```json
{
  "clarification_id": "fdf08c7d-1348-48ee-aa98-51fc1ca3b1d5",
  "revision": 1,
  "answers": { "client_id": "1" }
}
```

```json
{
  "success": true,
  "data": {
    "job_id": "...", "clarification_id": "...", "revision": 1, "lifecycle": "Queued"
  },
  "error": null
}
```

Job yang **sama** kembali mengantre; tidak ada job pengganti. `job.resumed`
belum dipancarkan di sini — ia muncul saat worker benar-benar melanjutkan.

**Skip** → `200`:

```json
{ "clarification_id": "...", "revision": 1, "action": "skip" }
```

```json
{
  "success": true,
  "data": {
    "job_id": "...", "clarification_id": "...", "revision": 1,
    "lifecycle": "Completed", "outcome": "SkippedByUser"
  },
  "error": null
}
```

`200`, bukan `202`: saat handler membalas, job sudah terminal dan response
document-nya sudah durable. Sesudah skip, job tidak dapat dilanjutkan —
jawaban berikutnya menghasilkan `409`.

`409` yang mungkin pada endpoint ini:

| `message` | Sebab |
| --- | --- |
| `Job is <lifecycle>, not waiting for an answer` | Job tidak `WaitingForUser` |
| `Clarification revision is stale; reload the active form` | `clarification_id`/`revision` tidak cocok form terbuka |
| `This clarification was already answered` | Pengiriman lain menang balapan |

---

## 7. SSE — `GET /chat/jobs/{job_id}/events`

`Content-Type: text/event-stream`. Bearer **dari header**, sehingga
`EventSource` bawaan browser tidak dapat dipakai; gunakan fetch-based SSE dan
kelola framing, cursor, reconnect dan backoff sendiri.

### Bentuk frame

```text
id: 3
event: job.phase_changed
data: {"schema_version":1,"sequence":3,"event":"job.phase_changed","occurred_at":"2026-09-15T03:32:34.416782Z","plan_version":1,"node_id":null,"node_attempt":null,"clarification_id":null,"clarification_revision":null,"response_version":null,"payload_truncated":false,"phase":"planning","message":"Preparing the analysis."}

```

`id` adalah `sequence`. Kirim kembali sebagai `Last-Event-ID` (atau query
`cursor`) saat menyambung ulang.

Field envelope selalu ada, bahkan saat `null`. Field spesifik event disebar ke
level atas dan **tidak pernah** menimpa field envelope.

### Kosakata event

| `event` | Pemicu | Menutup stream |
| --- | --- | --- |
| `job.accepted` | Job dibuat durable | — |
| `job.phase_changed` | Perpindahan fase publik | — |
| `job.resumed` | Worker **benar-benar** melanjutkan job yang ditangguhkan | — |
| `node.status_changed` | Node berubah status | — |
| `clarification.required` | Form aktif siap ditampilkan | — |
| `clarification.auto_resolved` | Seluruh slot terikat resolver tanpa bertanya (K5) | — |
| `clarification.accepted` | Jawaban valid commit | — |
| `job.notice` | Retry, delay, atau pembatasan cakupan | — |
| `job.completed` | Response tervalidasi durable | ✅ |
| `job.failed` | Kegagalan operasional durable | ✅ |
| `job.cancelled` | Pembatalan selesai | ✅ |
| `job.expired` | Kedaluwarsa selesai | ✅ |

### Fase publik

`queued`, `understanding`, `mapping_knowledge`, `planning`, `validating`,
`resolving_entities`, `querying`, `analyzing`, `composing_response`,
`validating_response`, `waiting_for_user`.

Fase yang benar-benar dipancarkan hari ini: `understanding` (saat worker
mengklaim) dan `planning` (saat plan terverifikasi). Fase lain menyusul bersama
tahap yang memancarkannya. Frontend wajib menganggap daftar fase sebagai
terbuka: fase yang tidak dikenal ditampilkan sebagai kemajuan generik, bukan
error.

Jangan mengubah jumlah node menjadi persentase waktu — re-plan dapat mengubah
jumlahnya.

### Reconnect

1. `GET /chat/jobs/{id}` untuk snapshot; ambil `last_event_sequence`.
2. Berlangganan dengan `Last-Event-ID: <cursor>`; replay dimulai **sesudah** itu.
3. Terapkan tiap `sequence` paling banyak sekali. **Duplikat aman, kehilangan
   tidak.**
4. Setelah event terminal, berhenti menyambung ulang. Koneksi yang tertutup
   sendirian **bukan** bukti terminal.

Cursor yang tidak sah gagal eksplisit, bukan diam-diam mulai dari nol:

| Kondisi | Response |
| --- | --- |
| `cursor` mendahului riwayat | `409` `Event cursor 999999 is ahead of this job (last committed sequence is 5); fetch a fresh job snapshot before subscribing` |
| Riwayat sebelum cursor sudah tidak disimpan | `409` `Event history before sequence N is no longer retained; ...` |
| `Last-Event-ID` bukan angka | `422` `Last-Event-ID must be an event sequence number` |

Menyambung ulang ke job yang **sudah terminal** dengan cursor di ujung akan
menutup stream seketika tanpa satu pun event — itu perilaku yang benar, bukan
koneksi gagal.

### Hal yang wajib diketahui frontend

- **Memutus koneksi tidak membatalkan job.** Gunakan endpoint cancel.
- Komentar keep-alive (`: keep-alive`, default tiap 15 detik) menjaga transport
  hidup. Ia **bukan** bukti worker masih bekerja.
- Pada `WaitingForUser`, frontend boleh menutup stream setelah menerima form,
  lalu berlangganan lagi dari cursor tersimpan setelah mengirim jawaban.
- Stream tidak pernah memuat SQL, prompt, atau stack trace.

---

## 8. Alur integrasi

### Pertanyaan yang langsung terjawab

```text
POST /chat/jobs                         → 202 { job_id, event_cursor }
GET  /chat/jobs/{id}/events             → job.accepted, job.phase_changed …, job.completed  (stream tutup)
GET  /chat/jobs/{id}/response           → dokumen final
```

### Pertanyaan yang butuh pilihan identitas

```text
POST /chat/jobs                         → 202
stream                                  → clarification.required   (lifecycle = WaitingForUser)
GET  /chat/jobs/{id}/clarification      → form; field single_choice
GET  …/clarification/options?field_id=… → halaman opsi (ulangi dengan cursor)
POST /chat/jobs/{id}/responses          → 202 { lifecycle: "Queued" }
stream (lanjut dari cursor)             → clarification.accepted, job.resumed, …, job.completed
GET  /chat/jobs/{id}/response           → dokumen final
```

### Slot terikat otomatis

Bila resolver hanya menemukan satu kandidat dalam scope, job **tidak pernah**
masuk `WaitingForUser`. Stream memancarkan `clarification.auto_resolved`, dan
response memuat blok `slots_auto_bound`. Frontend wajib menampilkannya:
"kebetulan hanya ada satu" berbeda secara material dari "pengguna memilih ini".

### Pengguna berhenti

```text
POST /chat/jobs/{id}/responses  { action: "skip" }  → 200 { lifecycle: "Completed", outcome: "SkippedByUser" }
GET  /chat/jobs/{id}/response                       → kind: "skipped", blok skipped_inputs
```

---

## 9. Yang belum ada

Endpoint dan field berikut **tidak** ada hari ini. Jangan dirancang ke dalam FE
seolah sudah ada:

- Dataset/handle berchunk dan pagination hasil besar.
- Blok `chart`, `findings`, `comparison`, `suggestions`.
- Narasi LLM. Seluruh teks hari ini deterministik.
- Konteks percakapan pada jawaban. `session_memory` sudah terisi saat commit,
  tetapi belum dibaca siapa pun: pertanyaan lanjutan masih dijawab berdiri
  sendiri, dan tidak ada field response yang mengungkap fakta yang dipakai.
- `refine_search` dan `change_intent` sebagai `answer_kind`.
- Klarifikasi bertahap (form kedua sesudah slot pertama terjawab).
- OpenAPI/JSON Schema formal.

Status lengkap: [build-order.md](../build-order.md).
