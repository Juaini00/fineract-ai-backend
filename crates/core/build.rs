// `sqlx::migrate!` menyematkan `migrations/` saat kompilasi. Tanpa ini, migrasi
// baru tidak ikut ter-embed pada build inkremental, dan app lokal yang sudah
// menerapkannya gagal startup ("previously applied but is missing").
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
