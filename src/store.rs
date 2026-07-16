//! Reading and writing per-message Markdown files under threads/<topic>/.

use crate::schema::{self, Message};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn thread_dir(root: &Path, topic: &str) -> PathBuf {
    root.join("threads").join(topic)
}

/// A sortable, readable, unique file path for a message:
/// threads/<topic>/<YYYYMMDDTHHMMSSZ>-<role>-<id-tail>.md
pub fn message_path(root: &Path, topic: &str, id: &str, role: &str, ts: &str) -> PathBuf {
    let compact: String = ts.chars().filter(char::is_ascii_alphanumeric).collect();
    let tail = if id.len() > 6 { &id[id.len() - 6..] } else { id };
    thread_dir(root, topic).join(format!("{compact}-{role}-{tail}.md"))
}

/// Messages ADDED since the `since` hub commit (exclusive), in git commit order.
/// `None` → the whole history. This is the incremental, commit-ordered read used
/// by the reactive loop (poll/watch); browse commands use `all_messages`.
pub fn messages_since(root: &Path, since: Option<&str>) -> Result<Vec<Message>> {
    let mut out = Vec::new();
    for f in crate::gitcmd::added_message_files(root, since)? {
        match std::fs::read_to_string(&f) {
            Ok(txt) => match schema::parse_message(&txt) {
                Ok(m) => out.push(m),
                Err(e) => eprintln!("confer: skipping {}: {e}", f.display()),
            },
            // Recorded in history but absent in the tree (sparse checkout, a concurrent GC, a partial
            // fetch). This is the REACTIVE path an unattended agent trusts to "see everything since
            // the cursor", so a silent skip could drop a request/reply with no trace — surface it,
            // matching `all_messages`'s warn-and-skip on its analogous case.
            Err(e) => eprintln!(
                "confer: ⚠ message {} is in history but unreadable in the tree ({e}) — it was NOT \
                 delivered this cycle; run `confer read` or re-fetch if you expected mail.",
                f.display()
            ),
        }
    }
    Ok(out)
}

/// All messages across all threads, in **id order** (the message id is a ULID whose leading
/// chars are a millisecond timestamp, so this is true time order — and correct for a last-wins
/// fold, unlike the second-precision filename). Unparseable files are logged and skipped.
pub fn all_messages(root: &Path) -> Result<Vec<Message>> {
    let dir = root.join("threads");
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut threads: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    threads.sort();
    for t in threads {
        if !t.is_dir() {
            continue;
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(&t)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        files.sort();
        for f in files {
            if f.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            // Skip-and-warn on an unreadable file (a concurrent GC/checkout,
            // permission glitch, transient FS error) — one bad file must not fail
            // the whole browse/triage command, matching `messages_since` (S1).
            let txt = match std::fs::read_to_string(&f) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("confer: skipping unreadable {}: {e}", f.display());
                    continue;
                }
            };
            match schema::parse_message(&txt) {
                Ok(m) => out.push(m),
                Err(e) => eprintln!("confer: skipping {}: {e}", f.display()),
            }
        }
    }
    // Fold order matters. The FILENAME is only second-precision with a RANDOM ULID tail, so
    // same-second events sort randomly and a last-wins fold gets them wrong (~50%) — the real
    // cause of the cross-clone claim/defer flake (a review finding). Re-sort by the full message
    // id: a ULID whose leading chars are a millisecond timestamp, so lexical order IS time order.
    // Separate `confer` invocations are ≥1ms apart (git ops between them), so causally-ordered
    // events (defer→claim) fold in the right order, deterministically on every clone.
    out.sort_by(|a, b| a.front.id.cmp(&b.front.id));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn tmp() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("confer-store-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn write_raw(root: &Path, topic: &str, filename: &str, id: &str) {
        let dir = thread_dir(root, topic);
        std::fs::create_dir_all(&dir).unwrap();
        // A minimal, valid message file whose id we pin explicitly.
        let txt = format!(
            "---\nid: {id}\nfrom: alice\ntype: note\nts: 2026-01-01T00:00:00Z\n---\n\nhi\n"
        );
        std::fs::write(dir.join(filename), txt).unwrap();
    }

    // Regression (external review #B): `all_messages` must fold in ID order, not filename
    // order. The on-disk filename is only SECOND-precision and its tiebreak is the ULID's
    // 6 RANDOM tail chars, so two same-second events sort randomly by filename — and a
    // last-wins projection fold then gets ~50% of same-second (defer→claim) pairs backwards.
    // Here the filename order is deliberately the OPPOSITE of the id order.
    #[test]
    fn all_messages_folds_in_id_order_not_filename_order() {
        let root = tmp();
        // Earlier id, but a filename tail that sorts LATE.
        write_raw(&root, "general", "20260101T000000Z-alice-ZZZZZZ.md", "01AAAAAAAAAAAAAAAAAAAAAAAA");
        // Later id, but a filename tail that sorts EARLY.
        write_raw(&root, "general", "20260101T000000Z-alice-AAAAAA.md", "01BBBBBBBBBBBBBBBBBBBBBBBB");

        let got = all_messages(&root).unwrap();
        let ids: Vec<&str> = got.iter().map(|m| m.front.id.as_str()).collect();
        // Filename order would yield [01B…(AAAAAA), 01A…(ZZZZZZ)]; id order is the correct one.
        assert_eq!(
            ids,
            vec!["01AAAAAAAAAAAAAAAAAAAAAAAA", "01BBBBBBBBBBBBBBBBBBBBBBBB"],
            "all_messages must return messages in ULID id order regardless of filename sort"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
