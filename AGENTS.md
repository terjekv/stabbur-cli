# Stabbur CLI contributor guidance

This repository builds the user-facing `stabbur` command. It pins one exact `stabbur_client`
version and server target. It contains no Stabbur HTTP paths, request construction, or direct
`reqwest` dependency.

## Verification

```text
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
cargo deny check
git ls-files -z '*.md' | xargs -0 npx --yes markdownlint-cli2@0.23.0 --config .markdownlint.json
```

CI builds Linux x86_64, macOS aarch64, and Windows x86_64 with stable, beta, and nightly. Release
jobs produce static Linux x86_64/aarch64, self-contained macOS ARM64, and static-CRT Windows x86_64
archives and SHA-256 checksums. Tags must point to a green main commit and match the manifest and
changelog.

## Architecture and security

- All API work goes through the blocking public client. Add missing behavior to
  `stabbur-client-rust`; never add `reqwest`, endpoint strings, bearer headers, or raw HTTP here.
- Keep command parsing, credential/profile handling, gateway calls, and output rendering separate
  enough for deterministic tests.
- Passwords and raw tokens are never positional or option values, Debug output, log fields, JSON
  output, or error text. Accept owner-only files, secure profiles, environment secrets, and
  interactive prompts.
- Default output is a concise human table. `--json` is stable automation output and changes to its
  field names require compatibility review.
- Destructive or publication operations require explicit confirmation unless a documented
  non-interactive flag is supplied.
- Stream downloads through the client, use resumable ranges, bound memory, and verify the final
  SHA-256 before reporting success.

Keep `Cargo.toml`, `COMPATIBILITY.md`, README, the client checkout tag in CI, and changelog aligned.
Run live workflows against the same immutable server image used by the pinned client before
claiming compatibility. Review the changelog for every pull request and document breaking changes
with migration guidance.

## Validated contracts

- Preserve validated facts as types across architectural boundaries. Convert raw API, file,
  command-line, and persistence representations once with a fallible constructor. Keep the
  resulting proof type private-fielded and pass it to downstream operations instead of rebuilding
  or rechecking the fact. Deserialization must use that constructor.
- Use newtypes for scalar invariants, enums for mutually exclusive or correlated states, and
  capability/proof wrappers for validated, resolved, authenticated, or planned state. Keep raw
  transport DTOs distinct from validated application values; do not expose mutable proof fields.
- Types complement database constraints, transactions, optimistic concurrency, and lease fencing.
  A validated snapshot cannot prove that mutable remote state remains current. Preserve revision
  and idempotency preconditions across calls and report stale state explicitly.
