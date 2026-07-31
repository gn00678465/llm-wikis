//! Step 9: prove full-content mutation detection (spec §8.1 step 17, plan
//! Task 2 Step 9). A temp synthetic wiki only — never touches D:\Wikis.

use crate::report::Report;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::SystemTime;

#[derive(Clone, PartialEq, Debug)]
struct FileFacts {
    hash: String,
    len: u64,
    mtime: SystemTime,
}

fn hash_file(path: &Path) -> String {
    let bytes = std::fs::read(path).expect("read file");
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("{:x}", hasher.finalize())
}

fn snapshot(root: &Path) -> BTreeMap<String, FileFacts> {
    let mut out = BTreeMap::new();
    for entry in walk(root) {
        let rel = entry
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let meta = std::fs::metadata(&entry).expect("metadata");
        out.insert(
            rel,
            FileFacts {
                hash: hash_file(&entry),
                len: meta.len(),
                mtime: meta.modified().expect("mtime"),
            },
        );
    }
    out
}

fn walk(root: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read_dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                out.push(path);
            }
        }
    }
    out
}

pub fn run() -> i32 {
    let mut r = Report::new("mutation-hash");

    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    std::fs::create_dir_all(root.join("pages")).expect("mkdir");
    std::fs::write(root.join("SCHEMA.md"), "schema v1\n").expect("write");
    std::fs::write(root.join("index.md"), "index\n\n[[alpha]]\n[[beta]]\n").expect("write");
    let target = root.join("pages").join("alpha.md");
    std::fs::write(&target, "Alpha page. Original content padded to a fixed length!!").expect("write");
    std::fs::write(root.join("pages").join("beta.md"), "Beta page, unrelated content.").expect("write");

    let before = snapshot(root);
    r.check("synthetic_wiki_snapshotted", !before.is_empty(), format!("{} files", before.len()));

    let before_facts = before.get("pages/alpha.md").cloned();

    // Same-length rewrite of exactly one file.
    let original = std::fs::read(&target).expect("read");
    let mut rewritten = original.clone();
    // Flip case of the first alphabetic byte so length is provably unchanged
    // but content differs.
    if let Some(b) = rewritten.iter_mut().find(|b| b.is_ascii_alphabetic()) {
        *b = if b.is_ascii_uppercase() { b.to_ascii_lowercase() } else { b.to_ascii_uppercase() };
    }
    let same_length = rewritten.len() == original.len();
    std::fs::write(&target, &rewritten).expect("rewrite");

    // Restore the original mtime.
    let before_mtime = before_facts.as_ref().unwrap().mtime;
    {
        let f = std::fs::OpenOptions::new().write(true).open(&target).expect("open for mtime restore");
        f.set_modified(before_mtime).expect("set_modified");
    }

    let after = snapshot(root);
    let after_facts = after.get("pages/alpha.md").cloned();

    let len_unchanged = before_facts.as_ref().unwrap().len == after_facts.as_ref().unwrap().len;
    let mtime_unchanged = before_facts.as_ref().unwrap().mtime == after_facts.as_ref().unwrap().mtime;
    let hash_changed = before_facts.as_ref().unwrap().hash != after_facts.as_ref().unwrap().hash;

    r.check("rewrite_preserved_byte_length", same_length && len_unchanged, format!("before_len={} after_len={}", before_facts.as_ref().unwrap().len, after_facts.as_ref().unwrap().len));
    r.check("rewrite_restored_mtime", mtime_unchanged, format!("before={:?} after={:?}", before_mtime, after_facts.as_ref().unwrap().mtime));
    r.check("rewrite_hash_differs_despite_equal_size_and_mtime", hash_changed, format!("before_hash={} after_hash={}", before_facts.as_ref().unwrap().hash, after_facts.as_ref().unwrap().hash));

    // Detection: compare full snapshots, report changed relative paths.
    let mut changed: Vec<&String> = Vec::new();
    for (path, facts) in &after {
        match before.get(path) {
            Some(b) if b.hash != facts.hash => changed.push(path),
            None => changed.push(path),
            _ => {}
        }
    }
    let detected_exactly_target = changed.len() == 1 && changed[0] == "pages/alpha.md";
    r.check(
        "mutation_detector_flags_exactly_the_changed_path",
        detected_exactly_target,
        format!("changed={changed:?}"),
    );

    r.finish()
}
