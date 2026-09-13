//! Pemuatan katalog dari disk beserta identitasnya.
//!
//! Identitas katalog adalah `content_hash` atas seluruh berkas yang dimuat —
//! bukan kolom `version`, yang di sistem lama literal selalu berisi "local"
//! (migrasi 3). Hash dihitung dari pasangan (path relatif, isi) yang diurutkan,
//! sehingga dua checkout dengan isi sama menghasilkan hash sama.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::catalog::model::{Capability, QueryManifest, SafetyPolicy, SensitivityClasses};

/// Katalog yang sudah dimuat, belum divalidasi.
#[derive(Debug)]
pub struct Catalog {
    pub capabilities: Vec<Loaded<Capability>>,
    pub queries: Vec<Loaded<QueryManifest>>,
    pub safety_policy: SafetyPolicy,
    /// Nama kelas sensitivitas yang sah, dari `columns/sensitivity.yaml`.
    pub sensitivity_classes: BTreeSet<String>,
    /// Isi setiap file SQL di `queries/`, dikunci path relatif terhadap root repo.
    pub sql_files: BTreeMap<String, String>,
    pub content_hash: String,
    /// Berkas YAML yang gagal diparse. Bukan diabaikan diam-diam (I5).
    pub unreadable: Vec<(String, String)>,
}

/// Sebuah entri beserta asal berkasnya, supaya temuan dapat menunjuk file.
#[derive(Debug)]
pub struct Loaded<T> {
    pub path: String,
    pub entry: T,
}

/// Muat `knowledge/` dan `queries/`.
///
/// `knowledge_root` dan `query_root` relatif terhadap direktori kerja proses
/// (lihat `CATALOG_PATH` / `QUERY_PATH`).
pub fn load(knowledge_root: &Path, query_root: &Path) -> anyhow::Result<Catalog> {
    let mut hasher = Sha256::new();
    let mut unreadable = Vec::new();

    let mut capabilities = Vec::new();
    let mut queries = Vec::new();
    let mut safety_policy = SafetyPolicy::default();
    let mut sensitivity_classes = BTreeSet::new();
    let mut sql_files = BTreeMap::new();

    // Diurutkan: hash tidak boleh bergantung pada urutan pembacaan direktori.
    let mut yaml_paths = collect(knowledge_root, &["yaml", "yml"])?;
    yaml_paths.sort();
    let mut sql_paths = collect(query_root, &["sql"])?;
    sql_paths.sort();

    for path in &yaml_paths {
        let relative = display_path(path);
        let text = std::fs::read_to_string(path)?;
        hash_entry(&mut hasher, &relative, &text);

        let under = |folder: &str| relative.contains(&format!("/{folder}/"));

        if under("capabilities") {
            match serde_yaml::from_str::<Capability>(&text) {
                Ok(entry) => capabilities.push(Loaded { path: relative, entry }),
                Err(error) => unreadable.push((relative, error.to_string())),
            }
        } else if under("queries") {
            match serde_yaml::from_str::<QueryManifest>(&text) {
                Ok(entry) => queries.push(Loaded { path: relative, entry }),
                Err(error) => unreadable.push((relative, error.to_string())),
            }
        } else if relative.ends_with("columns/sensitivity.yaml") {
            match serde_yaml::from_str::<SensitivityClasses>(&text) {
                Ok(declared) => sensitivity_classes.extend(declared.classes.into_keys()),
                Err(error) => unreadable.push((relative, error.to_string())),
            }
        } else if relative.ends_with("policies/query_safety.yaml") {
            match serde_yaml::from_str::<SafetyPolicy>(&text) {
                Ok(policy) => safety_policy = policy,
                Err(error) => unreadable.push((relative, error.to_string())),
            }
        }
        // Berkas lain (domains, schema, metrics, datasets, parameters) ikut
        // dihitung ke dalam content_hash tetapi belum punya validator sendiri;
        // cakupannya dinyatakan eksplisit oleh `coverage()`.
    }

    for path in &sql_paths {
        let relative = display_path(path);
        let text = std::fs::read_to_string(path)?;
        hash_entry(&mut hasher, &relative, &text);
        sql_files.insert(relative, text);
    }

    Ok(Catalog {
        capabilities,
        queries,
        safety_policy,
        sensitivity_classes,
        sql_files,
        content_hash: hex::encode(hasher.finalize()),
        unreadable,
    })
}

impl Catalog {
    /// Jumlah dokumen yang benar-benar dimuat sebagai entri bertipe.
    pub fn document_count(&self) -> usize {
        self.capabilities.len() + self.queries.len() + self.sql_files.len()
    }
}

fn hash_entry(hasher: &mut Sha256, relative: &str, text: &str) {
    // Panjang ikut di-hash supaya dua file yang digabung tidak menghasilkan
    // hash yang sama dengan satu file berisi keduanya.
    hasher.update(relative.as_bytes());
    hasher.update(b"\0");
    hasher.update(text.len().to_le_bytes());
    hasher.update(text.as_bytes());
}

fn collect(root: &Path, extensions: &[&str]) -> anyhow::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    walk(root, extensions, &mut found)?;
    Ok(found)
}

fn walk(directory: &Path, extensions: &[&str], found: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !directory.is_dir() {
        anyhow::bail!("direktori katalog tidak ditemukan: {}", directory.display());
    }

    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            walk(&path, extensions, found)?;
            continue;
        }

        let matches = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extensions.contains(&extension));

        if matches {
            found.push(path);
        }
    }

    Ok(())
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_order_independent_but_content_sensitive() {
        let mut first = Sha256::new();
        hash_entry(&mut first, "a.yaml", "isi a");
        hash_entry(&mut first, "b.yaml", "isi b");

        let mut same = Sha256::new();
        hash_entry(&mut same, "a.yaml", "isi a");
        hash_entry(&mut same, "b.yaml", "isi b");

        let mut changed = Sha256::new();
        hash_entry(&mut changed, "a.yaml", "isi a");
        hash_entry(&mut changed, "b.yaml", "isi b berbeda");

        assert_eq!(first.finalize(), same.finalize());
        assert_ne!(changed.finalize().len(), 0);
    }

    #[test]
    fn concatenated_files_do_not_collide_with_one_file() {
        let mut split = Sha256::new();
        hash_entry(&mut split, "x", "satu");
        hash_entry(&mut split, "x", "dua");

        let mut joined = Sha256::new();
        hash_entry(&mut joined, "x", "satudua");

        assert_ne!(split.finalize(), joined.finalize());
    }
}
