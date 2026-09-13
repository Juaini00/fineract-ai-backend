# Status: DIBAWA APA ADANYA, BELUM DIREVIEW

176 file YAML ini disalin dari repo `ai_report` pada 2026-09-13 **tanpa perubahan**.

**Ia belum menjadi sumber kebenaran.** Ia dibawa karena isinya pengetahuan domain yang mahal untuk disusun ulang, bukan karena sudah terbukti benar untuk desain baru.

## Yang belum diverifikasi

- **Kesesuaian dengan analytical contract Mode 2.** `data/analytical-contracts.md` belum ada. Sampai itu ditulis, tidak diketahui berapa banyak capability di sini yang perlu diekspresikan ulang sebagai kontrak + measure.
- **Prosa judul/deskripsi vs SQL yang dirujuk.** Pada sistem lama pernah ditemukan capability yang judulnya mengklaim lebih luas daripada query-nya, sehingga reranker membuatnya tidak terjangkau **tanpa satu pun kegagalan yang terlihat**. Bandingkan prosa dengan `.sql`-nya sebelum memercayai sebuah entri.
- **Klasifikasi sensitivitas kolom.** Sakelar PII global (#15) memutuskan *apakah* kolom berkelas `pii` dilepas; *kolom mana* yang berkelas `pii` dideklarasikan di sini. Klasifikasi ini harus diaudit ulang sebelum PII dinyalakan.
- **Contoh (`examples`) belum dijalankan ulang** terhadap data nyata pada schema baru.

## Aturan sebelum sebuah entri dianggap sah

1. Contohnya dijalankan ujung ke ujung dan angkanya diperiksa — memuat YAML dan `PREPARE` SQL **tidak membuktikan apa pun**.
2. Prosa dan SQL-nya konsisten.
3. Kelas sensitivitas kolom output-nya sesuai kebijakan PII yang berlaku.
4. Semantik cutoff/as-of-nya dideklarasikan ([database-design.md](../docs/data/database-design.md) §2.2 D4 dan keputusan #14).

Entri yang belum melewati empat hal di atas boleh dipakai untuk pengembangan lokal, **tidak** untuk menjawab pertanyaan yang dipercaya.
