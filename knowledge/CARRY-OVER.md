# Status: DIBAWA APA ADANYA, BELUM DIREVIEW

176 file YAML ini disalin dari repo `ai_report` pada 2026-09-13 **tanpa perubahan**.

**Ia belum menjadi sumber kebenaran.** Ia dibawa karena isinya pengetahuan domain yang mahal untuk disusun ulang, bukan karena sudah terbukti benar untuk desain baru.

## Yang belum diverifikasi

- **Kesesuaian dengan analytical contract Mode 2.** `data/analytical-contracts.md` belum ada. Sampai itu ditulis, tidak diketahui berapa banyak capability di sini yang perlu diekspresikan ulang sebagai kontrak + measure.
- **Prosa judul/deskripsi vs SQL yang dirujuk.** Pada sistem lama pernah ditemukan capability yang judulnya mengklaim lebih luas daripada query-nya, sehingga reranker membuatnya tidak terjangkau **tanpa satu pun kegagalan yang terlihat**. Bandingkan prosa dengan `.sql`-nya sebelum memercayai sebuah entri.
- **Klasifikasi sensitivitas kolom.** Sakelar PII global (#15) memutuskan *apakah* kolom berkelas `pii` dilepas; *kolom mana* yang berkelas `pii` dideklarasikan di sini. Klasifikasi ini harus diaudit ulang sebelum PII dinyalakan.
- **Contoh (`examples`) belum dijalankan ulang** terhadap data nyata pada schema baru.

## Sudah ditegakkan mekanis (sejak 2026-09-13)

`cargo run -p app -- catalog` memuat seluruh katalog, menghitung `content_hash`,
dan menjalankan check yang selama ini hanya hidup sebagai prosa di blok `checks:`
tiap file: keberadaan `query_id`, kecocokan parameter capability dengan query,
kelas sensitivitas yang benar-benar terdaftar, penegakan office scope sebagai
parameter terikat **di dalam** SQL, SELECT-only/single-statement/token terlarang,
kecocokan placeholder, serta — dengan menyiapkan tiap SQL pada schema Fineract
sungguhan — nama dan urutan kolom hasil terhadap `output_fields`.

Yang ditemukan pada pemeriksaan pertama dan sudah diperbaiki: kelas
`masked_output` dipakai empat manifest tanpa pernah dideklarasikan, dua manifest
savings tidak mendeklarasikan `guards` padahal SQL-nya mengikat scope, dan
delapan capability tanpa `display_name`/`description`.

## Aturan sebelum sebuah entri dianggap sah

1. Contohnya dijalankan ujung ke ujung dan angkanya diperiksa — memuat YAML dan `PREPARE` SQL **tidak membuktikan apa pun**. **Ini masih belum dikerjakan**; validator sengaja menyatakan batas itu pada bagian "Cakupan pemeriksaan" di keluarannya.
2. Prosa dan SQL-nya konsisten.
3. Kelas sensitivitas kolom output-nya sesuai kebijakan PII yang berlaku.
4. Semantik cutoff/as-of-nya dideklarasikan ([database-design.md](../docs/data/database-design.md) §2.2 D4 dan keputusan #14).

Entri yang belum melewati empat hal di atas boleh dipakai untuk pengembangan lokal, **tidak** untuk menjawab pertanyaan yang dipercaya.

## Cakupan: `data-scope/` dan `domains/` masih cakupan MVP lama (2026-09-26)

Empat aturan di atas membuktikan sebuah entri **benar**, bukan bahwa katalog
**cukup**. Katalog ini masih 49 kapabilitas pada 4 domain (client, savings,
organization, group), sedangkan cakupan rilis penuh ada di
[dataset-scope-decisions.md](../docs/product/2026-09-09-dataset-scope-decisions.md)
§1 dan D01–D15.

`data-scope/*.yaml` dan `domains/*.yaml` adalah cakupan **MVP** dari `ai_report`
dan **bukan otoritas**: `source_doc`-nya menunjuk `docs/reporting-data-scope.md`
dan `docs/reporting-data/*.md` yang tidak ada di repo ini, dan beberapa isinya
bertentangan dengan dokumen scope (trial balance vs D12, penolakan audit
pengguna vs D09, penolakan address vs §1 Client, tax `deferred` vs §1).
Bila bertentangan, dokumen scope yang menang. Penyelarasan = FIN-153; gerbang
cakupan = L1C di [build-order.md](../docs/build-order.md) §3.
