# Security Policy

confer is a security-focused tool — its purpose is verifiable, attributable coordination, and its
threat model is written up in [`DESIGN.md`](DESIGN.md). If you find a vulnerability, please report
it **privately** so it can be fixed before public disclosure.

## Reporting a vulnerability

Please **do not** open a public issue for a security vulnerability.

Instead, use GitHub's private vulnerability reporting — the **"Report a vulnerability"** button
under this repository's **Security** tab. (Maintainers: enable it under *Settings → Code security →
Private vulnerability reporting* if it isn't already on.)

Please include:
- what you found and the impact you believe it has,
- steps (or a minimal script) to reproduce it,
- the version / commit you tested.

We aim to acknowledge reports promptly, keep you updated on the fix, and credit you in the release
notes unless you'd prefer to remain anonymous.

## Scope

The most valuable reports concern the trust model: signature verification and TOFU key pinning
(`verify.rs`, `keyring.rs`), presence/heartbeat integrity (`presence.rs`), the untrusted-data
envelope and terminal-sanitization on rendered peer content, and anything that lets a hub writer
forge a `✓ verified` state or pass authority through a peer message. See `DESIGN.md` for the
boundaries confer does and does not claim to defend.

## Supported versions

confer is pre-1.0; security fixes land on the latest release. Pin a specific version if you need
stability, and watch releases for security updates.
