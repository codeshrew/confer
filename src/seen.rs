//! Real per-message read-receipts ("seen by"), for `/api/messages`.
//!
//! Mirrors `trust::cmd_seen`'s correctness model exactly: "role R has seen message M" iff M's
//! ADD-commit is an ancestor of R's published `presence::Presence.cursor` — and ONLY a
//! cryptographically SIGNED heartbeat counts (`BeatTrust::is_signed()`). An unsigned/forged
//! cursor must never manufacture a "seen" entry: if a beat's trust can't be established, or its
//! cursor can't be resolved, the role is simply OMITTED from `seenBy` (honest-by-omission, never
//! a false claim).
//!
//! Efficiency: rather than one `merge-base --is-ancestor` call per (message, role) pair — too
//! slow for an HTTP handler serving a busy hub — each SIGNED beat gets exactly one
//! `git rev-list <cursor>..HEAD` (the small set of commits that role has NOT yet consumed). A
//! message is seen by that role iff its ADD-commit is NOT in that set. The (message id → ADD
//! commit sha) map is itself expensive to build (a full `git log` over `threads/`), so it's
//! cached per (repo root, HEAD sha) and only rebuilt when HEAD moves (i.e. when new messages
//! land) — repeated `/api/messages` polls between arrivals cost ~zero extra git.

use crate::schema::Message;
use crate::{config, gitcmd, groups, presence, roster, store};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One confirmed read-receipt: `role` has consumed past the message, as of its heartbeat `ts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenBy {
    pub role: String,
    pub ts: String,
}

// ── ADD-commit map cache, keyed by (root, HEAD sha) ──────────────────────────────────────────

struct MapCache {
    root: PathBuf,
    head: String,
    rel_to_commit: HashMap<String, String>,
}

static MAP_CACHE: Mutex<Option<MapCache>> = Mutex::new(None);

fn rel_path(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Batch-build `threads/<...>.md` rel-path → ADD-commit sha over the whole hub, once per
/// distinct HEAD. One `git log --diff-filter=A --name-only` walk instead of a per-message
/// `git log -1 -- <path>` (what `trust::cmd_seen` does one message at a time — fine for a single
/// CLI lookup, too slow fanned out over an HTTP response with many messages).
fn adding_commit_map(root: &Path) -> HashMap<String, String> {
    let head = gitcmd::head(root).unwrap_or_default();
    {
        let cache = MAP_CACHE.lock().unwrap();
        if let Some(c) = cache.as_ref() {
            if c.root == root && c.head == head {
                return c.rel_to_commit.clone();
            }
        }
    }
    let mut map = HashMap::new();
    if let Ok(o) = gitcmd::output(
        root,
        &[
            "log",
            "--diff-filter=A",
            "--name-only",
            "--format=%x00%H",
            "--",
            "threads",
        ],
    ) {
        if o.status.success() {
            let text = String::from_utf8_lossy(&o.stdout);
            let mut cur_sha: Option<&str> = None;
            for line in text.split('\n') {
                if let Some(sha) = line.strip_prefix('\0') {
                    cur_sha = Some(sha.trim());
                } else if !line.is_empty() {
                    if let Some(sha) = cur_sha {
                        if line.starts_with("threads/") && line.ends_with(".md") {
                            // First commit to add a given path wins (there should only ever be
                            // one — messages are immutable — but if history is ever replayed,
                            // keep the earliest, matching "the commit that ADDED the file").
                            map.entry(line.to_string()).or_insert_with(|| sha.to_string());
                        }
                    }
                }
            }
        }
    }
    let mut cache = MAP_CACHE.lock().unwrap();
    *cache = Some(MapCache {
        root: root.to_path_buf(),
        head,
        rel_to_commit: map.clone(),
    });
    map
}

// ── audience (mirrors trust::cmd_seen exactly) ───────────────────────────────────────────────

fn audience_for(m: &Message, roster: &roster::Roster, grps: &groups::Groups) -> Vec<String> {
    let targets: Vec<&String> = m.front.to.iter().chain(m.front.cc.iter()).collect();
    let mut audience: Vec<String> = if targets.iter().any(|t| crate::is_reserved_name(t)) {
        roster.keys().cloned().collect()
    } else {
        targets
            .iter()
            .flat_map(|t| grps.get(*t).cloned().unwrap_or_else(|| vec![(*t).clone()]))
            .collect()
    };
    audience.retain(|r| r != &m.front.from);
    audience.sort();
    audience.dedup();
    audience
}

/// One signed role's usable read-frontier: the set of commits NEWER than its cursor (i.e. NOT
/// yet consumed). `None` when the cursor can't be resolved (unreachable/unknown) — "cannot
/// confirm", so the caller must omit the role entirely rather than guess.
struct Frontier {
    role: String,
    ts: String,
    unseen: HashSet<String>,
}

fn signed_frontiers(root: &Path, hub_key: &str, roster: &roster::Roster) -> Vec<Frontier> {
    presence::load_verified(root, hub_key, roster, false)
        .into_iter()
        .filter(|b| b.trust.is_signed())
        .filter_map(|b| {
            let cursor = b.p.cursor.as_deref()?;
            let o = gitcmd::output(root, &["rev-list", &format!("{cursor}..HEAD")]).ok()?;
            if !o.status.success() {
                return None; // unreachable cursor — can't confirm, omit
            }
            let unseen: HashSet<String> = String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            Some(Frontier { role: b.p.role.clone(), ts: b.p.last_seen.clone(), unseen })
        })
        .collect()
}

/// msgid → the addressed roles (with signed, resolvable cursors) who have consumed past that
/// message. A role appears under a msgid ONLY when confirmed seen: a signed beat, a locatable
/// ADD-commit, and that commit not in the role's unseen-set. Honest by omission — never a guess.
pub fn index(root: &Path, msgs: &[&Message], roster: &roster::Roster) -> HashMap<String, Vec<SeenBy>> {
    let mut out: HashMap<String, Vec<SeenBy>> = HashMap::new();
    if msgs.is_empty() {
        return out;
    }
    let grps = groups::load(root);
    let hub_key = config::hub_key(root);
    let frontiers = signed_frontiers(root, &hub_key, roster);
    if frontiers.is_empty() {
        return out;
    }
    let commit_map = adding_commit_map(root);

    for m in msgs {
        let topic = m.front.topic.as_deref().unwrap_or("general");
        let file = store::message_path(root, topic, &m.front.id, &m.front.from, &m.front.ts);
        let rel = rel_path(root, &file);
        let Some(msg_sha) = commit_map.get(&rel) else { continue }; // can't locate — omit entirely

        let audience = audience_for(m, roster, &grps);
        if audience.is_empty() {
            continue;
        }

        let mut seen_by: Vec<SeenBy> = frontiers
            .iter()
            .filter(|f| audience.contains(&f.role) && !f.unseen.contains(msg_sha))
            .map(|f| SeenBy { role: f.role.clone(), ts: f.ts.clone() })
            .collect();
        if seen_by.is_empty() {
            continue;
        }
        seen_by.sort_by(|a, b| a.role.cmp(&b.role));
        out.insert(m.front.id.clone(), seen_by);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::{AtomicU32, Ordering};

    // Serializes the tests below that mutate the process-wide $HOME env var (keyring pinning —
    // `presence::load_verified` → `verify::commit_trust` → `keyring::pin_or_check` — reads
    // `$HOME` with no dir-injection seam at that layer). Each test's keyring state is namespaced
    // under a per-repo `hub_key` (the root-commit sha) regardless, so this only guards against
    // literally racing the env mutation itself, not data collisions.
    static HOME_LOCK: Mutex<()> = Mutex::new(());

    fn tmp(tag: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("confer-seen-test-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn git(dir: &Path, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t.local", "-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
            .args(args)
            .output()
            .expect("run git")
    }

    fn ok(o: &std::process::Output) -> bool {
        if !o.status.success() {
            eprintln!("git failed: {}", String::from_utf8_lossy(&o.stderr));
        }
        o.status.success()
    }

    /// A fresh repo root, `git init`-ed with one throwaway root commit (so `config::hub_key`
    /// resolves).
    fn new_repo() -> PathBuf {
        let dir = tmp("repo");
        assert!(ok(&git(&dir, &["init", "-q"])));
        std::fs::write(dir.join("README.md"), "hub\n").unwrap();
        assert!(ok(&git(&dir, &["add", "README.md"])));
        assert!(ok(&git(&dir, &["commit", "-q", "-m", "init"])));
        dir
    }

    /// Write + commit a minimal valid message file under threads/general, returning its id.
    fn add_message(dir: &Path, id: &str, from: &str, to: &[&str]) -> String {
        let thread_dir = dir.join("threads").join("general");
        std::fs::create_dir_all(&thread_dir).unwrap();
        let ts = "2026-01-01T00:00:00Z";
        let to_yaml = if to.is_empty() {
            "[]".to_string()
        } else {
            format!("[{}]", to.join(", "))
        };
        let text = format!(
            "---\nid: {id}\nfrom: {from}\ntype: note\nts: {ts}\nto: {to_yaml}\n---\n\nhello\n"
        );
        let path = store::message_path(dir, "general", id, from, ts);
        std::fs::write(&path, text).unwrap();
        let rel = rel_path(dir, &path);
        assert!(ok(&git(dir, &["add", &rel])));
        assert!(ok(&git(dir, &["commit", "-q", "-m", &format!("msg {id}")])));
        id.to_string()
    }

    /// Generate an ed25519 keypair under `dir`, returning (privkey path, pubkey text).
    fn keygen(dir: &Path, name: &str) -> (PathBuf, String) {
        let key = dir.join(name);
        let status = Command::new("ssh-keygen")
            .args(["-t", "ed25519", "-f", key.to_str().unwrap(), "-N", "", "-C", name, "-q"])
            .status()
            .unwrap();
        assert!(status.success(), "ssh-keygen failed");
        let pubkey = std::fs::read_to_string(format!("{}.pub", key.display())).unwrap();
        (key, pubkey.trim().to_string())
    }

    /// Publish a presence beat for `role` at `cursor`/`last_seen`, signed with `key` iff `key`
    /// is `Some`. No push — `load_verified(fetch=false)` reads local `refs/presence/*` directly.
    fn push_beat(dir: &Path, role: &str, cursor: &str, last_seen: &str, key: Option<&Path>) {
        let json = format!(
            "{{\"role\":\"{role}\",\"last_seen\":\"{last_seen}\",\"cursor\":\"{cursor}\",\"poll_secs\":10}}"
        );
        let pres_path = dir.join(format!("pres-{role}.json"));
        std::fs::write(&pres_path, &json).unwrap();
        let hash_o = Command::new("git").arg("-C").arg(dir).args(["hash-object", "-w", pres_path.to_str().unwrap()]).output().unwrap();
        assert!(ok(&hash_o));
        let blob = String::from_utf8_lossy(&hash_o.stdout).trim().to_string();
        let tree_input = format!("100644 blob {blob}\tpresence.json\n");
        let mktree_o = Command::new("git").arg("-C").arg(dir).args(["mktree"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().and_then(|mut c| {
            use std::io::Write;
            c.stdin.take().unwrap().write_all(tree_input.as_bytes())?;
            c.wait_with_output()
        }).unwrap();
        assert!(ok(&mktree_o));
        let tree = String::from_utf8_lossy(&mktree_o.stdout).trim().to_string();
        let commit_o = if let Some(key) = key {
            let keygen_path = crate::ssh_keygen_path();
            Command::new("git")
                .arg("-C")
                .arg(dir)
                .args([
                    "-c", "gpg.format=ssh",
                    "-c", &format!("user.signingkey={}", key.display()),
                    "-c", &format!("gpg.ssh.program={keygen_path}"),
                    "commit-tree", &tree, "-S", "-m", "beat",
                ])
                .output()
                .unwrap()
        } else {
            Command::new("git").arg("-C").arg(dir).args(["commit-tree", &tree, "-m", "beat"]).output().unwrap()
        };
        assert!(ok(&commit_o));
        let commit = String::from_utf8_lossy(&commit_o.stdout).trim().to_string();
        let refname = format!("refs/presence/{role}");
        assert!(ok(&git(dir, &["update-ref", &refname, &commit])));
        let _ = std::fs::remove_file(&pres_path);
    }

    fn with_isolated_home(f: impl FnOnce()) {
        let _guard = HOME_LOCK.lock().unwrap();
        let old_home = std::env::var("HOME").ok();
        let home_dir = tmp("home");
        std::env::set_var("HOME", &home_dir);
        f();
        match old_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
        let _ = std::fs::remove_dir_all(&home_dir);
    }

    fn parse(dir: &Path, id: &str) -> Message {
        let all = store::all_messages(dir).unwrap();
        all.into_iter().find(|m| m.front.id == id).unwrap()
    }

    #[test]
    fn role_whose_cursor_covers_the_message_is_listed_with_its_ts() {
        with_isolated_home(|| {
            let dir = new_repo();
            let keydir = tmp("keys");
            let (key, pubkey) = keygen(&keydir, "bob");
            let mut ros: roster::Roster = HashMap::new();
            ros.insert("bob".into(), roster::Role { pubkey: Some(pubkey), ..Default::default() });

            let id = add_message(&dir, "01AAAAAAAAAAAAAAAAAAAAAAAA", "alice", &["bob"]);
            let head = gitcmd::head(&dir).unwrap();
            // bob's cursor is HEAD itself — covers the message.
            push_beat(&dir, "bob", &head, "2026-01-01T01:00:00Z", Some(&key));

            let m = parse(&dir, &id);
            let idx = index(&dir, &[&m], &ros);
            let seen = idx.get(&id).expect("bob should be listed as having seen it");
            assert_eq!(seen.len(), 1);
            assert_eq!(seen[0].role, "bob");
            assert_eq!(seen[0].ts, "2026-01-01T01:00:00Z");
        });
    }

    #[test]
    fn role_whose_cursor_is_behind_the_message_is_not_listed() {
        with_isolated_home(|| {
            let dir = new_repo();
            let keydir = tmp("keys");
            let (key, pubkey) = keygen(&keydir, "bob");
            let mut ros: roster::Roster = HashMap::new();
            ros.insert("bob".into(), roster::Role { pubkey: Some(pubkey), ..Default::default() });

            // bob's cursor is BEFORE the message lands.
            let stale_cursor = gitcmd::head(&dir).unwrap();
            let id = add_message(&dir, "01BBBBBBBBBBBBBBBBBBBBBBBB", "alice", &["bob"]);
            push_beat(&dir, "bob", &stale_cursor, "2026-01-01T01:00:00Z", Some(&key));

            let m = parse(&dir, &id);
            let idx = index(&dir, &[&m], &ros);
            assert!(!idx.contains_key(&id), "bob's cursor predates the message — must not be listed");
        });
    }

    #[test]
    fn unsigned_forged_beat_is_not_trusted_even_if_its_cursor_covers_the_message() {
        with_isolated_home(|| {
            let dir = new_repo();
            let keydir = tmp("keys");
            let (_key, pubkey) = keygen(&keydir, "bob");
            let mut ros: roster::Roster = HashMap::new();
            ros.insert("bob".into(), roster::Role { pubkey: Some(pubkey), ..Default::default() });

            let id = add_message(&dir, "01CCCCCCCCCCCCCCCCCCCCCCCC", "alice", &["bob"]);
            let head = gitcmd::head(&dir).unwrap();
            // Forged: an UNSIGNED beat claiming a cursor that covers the message, for a role
            // that publishes a signing key (so it's expected to sign) — must be rejected, not
            // merely advisory.
            push_beat(&dir, "bob", &head, "2026-01-01T01:00:00Z", None);

            let m = parse(&dir, &id);
            let idx = index(&dir, &[&m], &ros);
            assert!(
                !idx.contains_key(&id),
                "an unsigned/forged cursor must never produce a seen receipt (trust gate)"
            );
        });
    }

    #[test]
    fn message_with_no_audience_yields_empty() {
        with_isolated_home(|| {
            let dir = new_repo();
            let keydir = tmp("keys");
            let (key, pubkey) = keygen(&keydir, "bob");
            let mut ros: roster::Roster = HashMap::new();
            ros.insert("bob".into(), roster::Role { pubkey: Some(pubkey), ..Default::default() });

            // No `to`/`cc` at all — nobody addressed.
            let id = add_message(&dir, "01DDDDDDDDDDDDDDDDDDDDDDDD", "alice", &[]);
            let head = gitcmd::head(&dir).unwrap();
            push_beat(&dir, "bob", &head, "2026-01-01T01:00:00Z", Some(&key));

            let m = parse(&dir, &id);
            let idx = index(&dir, &[&m], &ros);
            assert!(!idx.contains_key(&id), "a message with no audience must yield no seenBy entries");
        });
    }
}
