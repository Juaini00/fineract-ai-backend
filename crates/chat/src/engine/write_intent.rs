//! Guard intent tulis (OVR-6.6, FIN-139 — keputusan owner: deterministik).
//!
//! Jarvis read-only terhadap Fineract. Permintaan yang menyuruh MENGUBAH data
//! ditolak sebelum retrieval, plan, dan query sumber apa pun — bukan
//! dicocokkan ke capability baca terdekat lalu dijawab seolah didukung.
//!
//! Dua pola, keduanya sengaja sempit supaya pertanyaan baca tidak ikut
//! tertolak (salah tolak adalah bug yang sama nyatanya):
//!
//! 1. **Perintah atas entitas Fineract**: token bermakna pertama (sesudah
//!    sapaan seperti "please", "tolong") adalah kata kerja mutasi DAN sebuah
//!    entitas Fineract (client, rekening, pinjaman, …) muncul dalam beberapa
//!    token sesudahnya. "Delete all clients" ditolak; "Change the period to
//!    last quarter", "Update saldo tabungan per kantor" dan "Create a chart of
//!    deposits" tidak. Kata benda domain yang juga kata kerja (deposit,
//!    transfer, tarik, withdraw) sengaja tidak dihitung sebagai perintah.
//! 2. **SQL tulis/DDL** di mana pun dalam teks: `create|drop|truncate|alter`
//!    diikuti objek DDL, `delete from`, `insert into`, `update … set`, serta
//!    `grant|revoke` diikuti hak akses.

/// Sapaan/pembuka yang dilewati sebelum menilai kata kerja pertama.
const LEADING_FILLER: &[&str] = &[
    "please", "pls", "kindly", "can", "could", "would", "will", "you", "jarvis", "hey", "hi",
    "tolong", "mohon", "bisa", "bisakah", "coba", "silakan",
];

/// Kata kerja mutasi (EN/ID). Hanya dihitung bila diikuti entitas Fineract.
const MUTATION_VERBS: &[&str] = &[
    "delete",
    "remove",
    "erase",
    "purge",
    "update",
    "modify",
    "change",
    "edit",
    "rename",
    "insert",
    "add",
    "create",
    "make",
    "open",
    "reopen",
    "close",
    "drop",
    "truncate",
    "alter",
    "approve",
    "reject",
    "disburse",
    "activate",
    "deactivate",
    "reverse",
    "undo",
    "assign",
    "unassign",
    "waive",
    "hapus",
    "hapuskan",
    "hapuslah",
    "ubah",
    "ubahlah",
    "ganti",
    "gantikan",
    "perbarui",
    "perbaharui",
    "tambah",
    "tambahkan",
    "masukkan",
    "buat",
    "buatkan",
    "buka",
    "setujui",
    "tolak",
    "cairkan",
    "tutup",
    "aktifkan",
    "nonaktifkan",
    "batalkan",
];

/// Kata yang dilewati di antara kata kerja dan objeknya.
const DETERMINERS: &[&str] = &[
    "a", "an", "the", "all", "every", "each", "this", "these", "that", "those", "my", "new",
    "semua", "seluruh", "setiap", "sebuah", "satu", "baru", "ini", "itu",
];

/// Entitas Fineract yang dapat menjadi objek perintah tulis.
const ENTITIES: &[&str] = &[
    "client",
    "clients",
    "customer",
    "customers",
    "account",
    "accounts",
    "loan",
    "loans",
    "office",
    "offices",
    "branch",
    "branches",
    "group",
    "groups",
    "center",
    "centers",
    "staff",
    "user",
    "users",
    "transaction",
    "transactions",
    "product",
    "products",
    "charge",
    "charges",
    "gl",
    "nasabah",
    "klien",
    "rekening",
    "akun",
    "pinjaman",
    "kantor",
    "cabang",
    "kelompok",
    "transaksi",
    "produk",
    "biaya",
    "pengguna",
    "petugas",
];

/// Jendela token (sesudah determiner dibuang) tempat objek harus muncul.
const OBJECT_WINDOW: usize = 3;

/// Objek DDL setelah `create|drop|truncate|alter`.
const DDL_OBJECTS: &[&str] = &[
    "table", "database", "schema", "column", "index", "view", "user", "role", "function", "trigger",
];

/// Hak akses setelah `grant|revoke`.
const PRIVILEGES: &[&str] = &[
    "select", "insert", "update", "delete", "all", "usage", "execute",
];

pub fn is_write_request(text: &str) -> bool {
    let tokens: Vec<String> = text
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .collect();

    command_on_entity(&tokens) || sql_write(&tokens)
}

fn command_on_entity(tokens: &[String]) -> bool {
    let mut rest = tokens
        .iter()
        .skip_while(|token| LEADING_FILLER.contains(&token.as_str()));

    let Some(verb) = rest.next() else {
        return false;
    };
    if !MUTATION_VERBS.contains(&verb.as_str()) {
        return false;
    }

    rest.filter(|token| !DETERMINERS.contains(&token.as_str()))
        .take(OBJECT_WINDOW)
        .any(|token| ENTITIES.contains(&token.as_str()))
}

fn sql_write(tokens: &[String]) -> bool {
    tokens.iter().enumerate().any(|(index, token)| {
        let next = tokens.get(index + 1).map(String::as_str);
        match token.as_str() {
            "create" | "drop" | "truncate" | "alter" => {
                next.is_some_and(|n| DDL_OBJECTS.contains(&n))
            }
            "delete" => next == Some("from"),
            "insert" => next == Some("into"),
            "update" => tokens[index + 1..].iter().any(|t| t == "set"),
            "grant" | "revoke" => next.is_some_and(|n| PRIVILEGES.contains(&n)),
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::is_write_request;

    #[test]
    fn commands_that_change_fineract_data_are_writes() {
        for text in [
            "Delete all clients.",
            "Please update the name of client 1 to Mallory.",
            "tolong hapus semua nasabah",
            "Ubah saldo rekening 5 menjadi 0",
            "Approve all pending loans",
            "Create savings account for client 5",
            "Buka rekening tabungan untuk nasabah 5",
            "Close account 12",
            "Run this SQL: DROP TABLE m_client",
            "Run this SQL: CREATE TABLE x (id int)",
            "execute: delete from m_client where id = 1",
            "can you run UPDATE m_client SET display_name = 'x'",
            "GRANT ALL ON m_client TO mallory",
        ] {
            assert!(is_write_request(text), "{text}");
        }
    }

    /// Salah tolak pertanyaan baca adalah bug yang sama nyatanya — termasuk
    /// kata benda domain yang juga kata kerja, dan follow-up percakapan.
    #[test]
    fn reads_and_follow_ups_are_not_writes() {
        for text in [
            "Show the savings portfolio summary.",
            "How many clients were deleted last month?",
            "List closed savings accounts",
            "Top withdrawals per month this year",
            "Create a chart of deposits by office",
            "Buatkan ringkasan saldo tabungan per cabang",
            "Deposit volume per office this month",
            "Transfer masuk terbesar minggu ini",
            "Tarik data setoran bulan ini",
            "Change in savings balance since last month",
            "Add up deposits by office",
            "Write a short summary of this month's deposits",
            "Update saldo tabungan per kantor dong",
            "Could you change the period to last quarter?",
            "Ganti periodenya ke bulan lalu",
            "Grant-funded savings accounts per office",
            "Show the table of deposits",
        ] {
            assert!(!is_write_request(text), "{text}");
        }
    }
}
