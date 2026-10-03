# Changelog

## Unreleased

- Add exports list/show/save/plan/apply/history/download/profile/revoke-profiles using the shared client. Downloads verify complete snapshots into a new directory and preserve existing Munki manifest management. New commands require the coordinated development server.

## [0.0.1] - 2026-09-26

- Target the first coordinated release as 0.0.1.
- Gate immutable GitHub source/package releases on exact main CI and published server-image evidence.
- Update rustls to 0.23.45 to address RUSTSEC-2026-0285 without advisory exceptions.

### Fixed

- Fix Munki exports to accept the server's `mac_os` and `aarch64` values and translate ARM metadata
  to Munki's `arm64`. Resolver arguments now preserve validated platform and concrete CPU enums;
  `macos` and `arm64` remain accepted CLI aliases. Unknown values and `universal` targets fail at parsing.

### Added

- Add `catalog import --snapshot ID --selections FILE --output FILE` to prepare a validated catalog for plan/sync.

- Add `target trigger --watch`, workflow help examples, and optional current-channel revision
  lookup for promotion, while retaining concurrency fencing and automation defaults.

- Add validated catalog schema 2 with exact revision/append preconditions, source-pin proposals,
  software/queue summaries, worker draining and release withdrawal.
- Add bounded typed SSE decoding with reconnect/replay and hard run deadlines.
- Preserve list pagination in JSON and add `--all`; report failed/cancelled/deadline run exits.
- Verify artifact downloads in temporary files before replacing destinations, including resume.
- Export resolved macOS installers and reviewed Munki pkginfo with verified hashes.

- Add protected-file and interactive first-administrator bootstrap commands at `stabbur
  bootstrap` and `stabbur auth bootstrap`, without requiring an existing bearer credential.
- Add `stabbur auth reset-password` as a shorter authenticated alias for human-principal password
  reset while retaining the resource-nested command.
- Add versioned `catalog plan` and `catalog sync` commands with deterministic normalization,
  additive reconciliation, optimistic software updates, and changed-revision detection.
- Add catalog snapshot inspection, exact recipe lookup, and durable AutoPkg scan
  request/list/show/cancel commands.
- Add desired build-target list/show/create/update/trigger/run-history commands for manual and
  fixed-interval policy.

### Changed

- Render readable nested human output, width-aware tables, local timestamps and actionable
  field errors. Existing JSON response and error envelopes are unchanged.

- Breaking: list JSON is a cursor envelope; use `.items` rather than treating it as an array.

- Make `recipe create-revision` consume the builder-neutral revision envelope, including the
  deterministic `fake` builder used by unattended end-to-end tests.
- Fail protected-input prompts immediately when no interactive terminal is available, and preserve
  intentional password-file whitespace while removing only line endings.

## Initial development - 2026-08-26

### Added

- Complete one-shot command coverage for every Stabbur 0.0.1 public resource.
- Principal, password, token, role, software, release, variant, channel, recipe revision, run,
  artifact, store, worker, job, audit, and resolver workflows through the exact blocking client.
- ETag revisions, idempotency keys, explicit confirmation gates, live SSE logs, bounded streaming
  uploads, resumable SHA-256-verified downloads, and stable JSON output.
- Owner-only profiles, API-token outputs, and server-issued worker credential JSON files.
- Cross-platform CI and release artifacts for static Linux x86_64/aarch64, macOS ARM64, and
  static-CRT Windows x86_64.
- Document explicit worker-local AutoPkg executable selection, receipt provenance, trust modes,
  and private-path redaction for remote packagers.
