//! The typed exit-code markers `main` routes on.
//!
//! Confer's exit-code contract (DESIGN.md): 0 = success / report produced / predicate YES;
//! 1 = predicate NO (a valid negative, ONLY from predicate commands); 2 = usage (clap), the
//! Stop-hook block, `apply --check`'s "already landed", or `update --check`'s "cannot determine";
//! 3 = execution/environment error.
//!
//! Each of these is a VALID outcome travelling home through the `Result` channel — never an error
//! in the ordinary sense — so a handler can print its report and then `return` the marker. That is
//! what keeps the promise made in `main`: codes return UP through one place and there is never a
//! mid-stack `process::exit`, which would skip `Drop` on clone locks and cursor state.

/// A predicate command's valid NEGATIVE result — e.g. `watch-status --check` on an unhealthy watcher,
/// `verify` on a key mismatch. NOT an error: it maps to exit code 1 in `main`, distinct from an
/// execution failure (exit 3). Carried through the `Result` channel so a predicate handler can `return`
/// it AFTER printing its report, without a mid-stack `process::exit` (which would skip `Drop` on locks).
#[derive(Debug)]
pub(crate) struct PredicateFalse;
impl std::fmt::Display for PredicateFalse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("predicate not satisfied")
    }
}
impl std::error::Error for PredicateFalse {}

/// The Claude Code Stop-hook "block the stop" signal (`poll --hook` with new mail): exit code 2, payload
/// already on stderr for the model. An ADAPTER contract imposed by the host, not confer's own scheme —
/// carried through `Result` like `PredicateFalse` so there's no mid-stack `process::exit`.
#[derive(Debug)]
pub(crate) struct StopHookBlock;
impl std::fmt::Display for StopHookBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("stop-hook: new mail")
    }
}
impl std::error::Error for StopHookBlock {}

/// `confer apply --check`'s distinct "already landed" verdict (design/45 §1.5, design/37 exit
/// vocabulary: 0 applies cleanly, 1 conflicts, 2 already landed, 3 unresolvable) — landing isn't a
/// failure, but it IS distinct from "would apply cleanly" for a scriptable caller, so it gets its
/// own code (2) rather than overloading `PredicateFalse`'s 1.
#[derive(Debug)]
pub(crate) struct AlreadyLanded;
impl std::fmt::Display for AlreadyLanded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("already landed")
    }
}
impl std::error::Error for AlreadyLanded {}

/// A predicate command's "I cannot determine this" verdict — currently `update --check` when the
/// running binary isn't dist-managed (no receipt, or a receipt describing a DIFFERENT install). It is
/// neither YES nor NO: reporting it as either is the bug this exists to prevent (an instrument that
/// can't say "I don't know" reports a confident wrong answer). Gets its own code (2) rather than
/// overloading `PredicateFalse`'s 1, exactly as `apply --check`'s "already landed" does.
#[derive(Debug)]
pub(crate) struct Indeterminate;
impl std::fmt::Display for Indeterminate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cannot determine")
    }
}
impl std::error::Error for Indeterminate {}
