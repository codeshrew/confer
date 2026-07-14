//! Build identity + hub-pin comparison for update detection.
//!
//! Semantic-version parsing/ordering comes from the standard [`semver`] crate (the
//! one Cargo uses) — we don't hand-roll it. On top we add the confer-specific bits:
//! grading a version gap `major`/`minor`/`patch` (`Drift`), and comparing an agent's
//! BUILT id against the hub's pin.
//!
//! A build is `(semver, sha)`. The hub records the version it expects in
//! `.confer-version` as `"<semver> <sha>"` (legacy pins are a bare `"<sha>"`). The
//! verdict is a single grade a consumer can branch on: `current`/`ahead` (fine) ·
//! `rebuild` (same version, newer build) · `patch`/`minor`/`major` (behind by that
//! much) · `drift` (legacy sha-only mismatch).

use semver::{Version, VersionReq};

/// Does a build's semver satisfy a hub REQUIREMENT floor/range (a `VersionReq` like
/// `>=0.1.0`)? The "fuzzy repo-level" version the whole hub declares; each agent reports
/// its exact build, and this is the per-agent compatibility check (a build with no
/// parseable semver never satisfies — treated as incompatible/unknown).
pub fn satisfies(build: &BuildId, req: &VersionReq) -> bool {
    build.version.as_ref().is_some_and(|v| req.matches(v))
}

/// The lowest semver among a set of builds (for the "safe to raise the floor to X once
/// everyone is at least X" auto-bump). `None` if no build has a parseable semver.
pub fn min_version(builds: &[BuildId]) -> Option<Version> {
    builds.iter().filter_map(|b| b.version.clone()).min()
}

/// How far a BUILT version is from the hub PIN — graded so a consumer can branch:
/// `Major`/`Minor`/`Patch` = behind (act-now → later → low-noise); `Ahead` = newer
/// than the pin; `Current` = exactly the pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drift {
    Current,
    Ahead,
    Patch,
    Minor,
    Major,
}

impl Drift {
    /// Grade a built version against the pin. When behind, the grade is the *highest*
    /// differing core component (a major gap dominates a coincident minor/patch gap).
    /// Behind by only a pre-release (same core, built is `-rc`) grades `Patch`.
    pub fn grade(built: &Version, pin: &Version) -> Drift {
        use std::cmp::Ordering::*;
        match built.cmp(pin) {
            Equal => Drift::Current,
            Greater => Drift::Ahead,
            Less => {
                if built.major != pin.major {
                    Drift::Major
                } else if built.minor != pin.minor {
                    Drift::Minor
                } else {
                    Drift::Patch
                }
            }
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Drift::Current => "current",
            Drift::Ahead => "ahead",
            Drift::Patch => "patch",
            Drift::Minor => "minor",
            Drift::Major => "major",
        }
    }
}

/// A confer build: an optional semver (unparseable/legacy → `None`) + a short git sha.
#[derive(Clone)]
pub struct BuildId {
    pub version: Option<Version>,
    pub sha: String,
}

impl BuildId {
    /// Parse a pin/build token: `"<semver> <sha>"` | `"<semver>"` | `"<sha>"`.
    pub fn parse(s: &str) -> BuildId {
        let s = s.trim();
        let mut parts = s.split_whitespace();
        let first = parts.next().unwrap_or("");
        match Version::parse(first) {
            Ok(v) => BuildId { version: Some(v), sha: parts.next().unwrap_or("").to_string() },
            Err(_) => BuildId { version: None, sha: first.to_string() }, // legacy sha-only pin
        }
    }

    /// Core `major.minor.patch[-pre]` without build metadata (what we pin/compare on).
    fn core(v: &Version) -> String {
        if v.pre.is_empty() {
            format!("{}.{}.{}", v.major, v.minor, v.patch)
        } else {
            format!("{}.{}.{}-{}", v.major, v.minor, v.patch, v.pre)
        }
    }

    /// The canonical `"<semver> <sha>"` form written into a pin.
    pub fn pin_string(&self) -> String {
        match &self.version {
            Some(v) if !self.sha.is_empty() => format!("{} {}", Self::core(v), self.sha),
            Some(v) => Self::core(v),
            None => self.sha.clone(),
        }
    }

    /// Human label, e.g. `0.2.0 (67a1148)`.
    pub fn label(&self) -> String {
        match (&self.version, self.sha.is_empty()) {
            (Some(v), false) => format!("{} ({})", Self::core(v), self.sha),
            (Some(v), true) => Self::core(v),
            (None, false) => self.sha.clone(),
            (None, true) => "unknown".to_string(),
        }
    }
}

/// The graded relationship of a built id to the hub pin.
pub struct Assessment {
    /// `no-pin` | `current` | `ahead` | `rebuild` | `patch` | `minor` | `major` | `drift`.
    pub grade: &'static str,
    /// Behind the pin (an update is available) → callers exit non-zero.
    pub outdated: bool,
}

/// Grade a built id against the hub pin (if any). Prefers semver grading; falls back
/// to a bare sha compare for legacy pins. Same semver + different sha = a `rebuild`
/// (a newer build to adopt even without a version bump — the common case today).
pub fn assess(built: &BuildId, pin: Option<&BuildId>) -> Assessment {
    let Some(pin) = pin else {
        return Assessment { grade: "no-pin", outdated: false };
    };
    match (&built.version, &pin.version) {
        (Some(b), Some(p)) => {
            use std::cmp::Ordering::*;
            match b.cmp(p) {
                Greater => Assessment { grade: "ahead", outdated: false },
                Less => Assessment { grade: Drift::grade(b, p).label(), outdated: true },
                Equal => {
                    if built.sha == pin.sha {
                        Assessment { grade: "current", outdated: false }
                    } else {
                        Assessment { grade: "rebuild", outdated: true }
                    }
                }
            }
        }
        // No comparable semver on at least one side → fall back to sha identity.
        _ => {
            if pin.sha.is_empty() {
                Assessment { grade: "no-pin", outdated: false }
            } else if built.sha == pin.sha {
                Assessment { grade: "current", outdated: false }
            } else {
                Assessment { grade: "drift", outdated: true }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ver(s: &str) -> Version {
        Version::parse(s).unwrap()
    }
    fn b(s: &str) -> BuildId {
        BuildId::parse(s)
    }

    #[test]
    fn drift_grades_by_highest_differing_component() {
        use Drift::*;
        assert_eq!(Drift::grade(&ver("1.2.3"), &ver("1.2.3")), Current);
        assert_eq!(Drift::grade(&ver("1.3.0"), &ver("1.2.9")), Ahead);
        assert_eq!(Drift::grade(&ver("1.2.3"), &ver("1.2.4")), Patch);
        assert_eq!(Drift::grade(&ver("1.2.9"), &ver("1.3.0")), Minor);
        assert_eq!(Drift::grade(&ver("1.9.9"), &ver("2.0.0")), Major);
        assert_eq!(Drift::grade(&ver("1.2.3"), &ver("2.3.4")), Major); // major dominates
        assert_eq!(Drift::grade(&ver("1.0.0-rc.1"), &ver("1.0.0")), Patch); // pre-release behind
    }

    #[test]
    fn parses_pin_forms() {
        let x = b("0.2.0 67a1148");
        assert_eq!(x.version.as_ref().unwrap().to_string(), "0.2.0");
        assert_eq!(x.sha, "67a1148");
        assert!(b("abc1234").version.is_none()); // legacy sha-only (not a semver)
        assert_eq!(b("abc1234").sha, "abc1234");
        assert_eq!(b("1.0.0").sha, "");
        assert_eq!(b("0.2.0 abc").pin_string(), "0.2.0 abc");
    }

    #[test]
    fn assess_grades_every_case() {
        let g = |built: &str, pin: Option<&str>| {
            let pinb = pin.map(b);
            assess(&b(built), pinb.as_ref())
        };
        assert_eq!(g("0.1.0 aaa", None).grade, "no-pin");
        let cur = g("0.2.0 aaa", Some("0.2.0 aaa"));
        assert_eq!(cur.grade, "current");
        assert!(!cur.outdated);
        assert_eq!(g("0.3.0 zzz", Some("0.2.0 aaa")).grade, "ahead");
        let r = g("0.2.0 bbb", Some("0.2.0 aaa"));
        assert_eq!(r.grade, "rebuild");
        assert!(r.outdated);
        assert_eq!(g("0.2.0 x", Some("0.2.4 y")).grade, "patch");
        assert_eq!(g("0.2.0 x", Some("0.5.0 y")).grade, "minor");
        assert_eq!(g("0.2.0 x", Some("1.0.0 y")).grade, "major");
        assert!(g("0.2.0 x", Some("1.0.0 y")).outdated);
        // legacy sha-only pins
        assert_eq!(g("0.1.0 aaa", Some("aaa")).grade, "current");
        assert_eq!(g("0.1.0 aaa", Some("bbb")).grade, "drift");
        assert!(g("0.1.0 aaa", Some("bbb")).outdated);
    }

    #[test]
    fn floor_satisfaction_and_min() {
        let req = VersionReq::parse(">=0.2.0").unwrap();
        assert!(satisfies(&b("0.2.0 aaa"), &req));
        assert!(satisfies(&b("0.3.1 aaa"), &req));
        assert!(!satisfies(&b("0.1.9 aaa"), &req));
        assert!(!satisfies(&b("abc1234"), &req)); // no semver → not compatible
        let builds = [b("0.2.0 a"), b("0.3.0 b"), b("0.1.5 c")];
        assert_eq!(min_version(&builds).unwrap().to_string(), "0.1.5");
    }
}
