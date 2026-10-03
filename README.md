# Stabbur CLI

`stabbur` is the primary operational interface for Stabbur 0.0.1. It pins
`stabbur_client = 0.0.1` exactly, has no direct HTTP dependency, and performs every API operation
through the blocking supported client.

```bash
stabbur bootstrap --username admin \
  --bootstrap-secret-file /run/secrets/stabbur-bootstrap \
  --password-file /run/secrets/stabbur-admin-password
stabbur auth login --username operator
stabbur --json catalog plan --file catalog.json
stabbur --json catalog sync --file catalog.json
stabbur catalog scan request --source-url https://example.net/recipes.git \
  --source-revision 0123456789abcdef0123456789abcdef01234567 \
  --idempotency-key recipes-0123456789abcdef
stabbur target create --name firefox-manual --software firefox \
  --recipe-revision RECIPE_REVISION_ID
stabbur target trigger firefox-manual --idempotency-key firefox-manual-1
stabbur software list
stabbur software show firefox
stabbur resolve firefox --channel stable --platform mac_os --architecture aarch64 --macos 15.0
stabbur artifact download SHA256 --output Firefox.pkg --resume
```

Human-readable tables are the default, with complete labeled records when a table exceeds the
terminal width (`COLUMNS`, default 120). Nested results and field errors have readable labels;
timestamps include the local timezone. Add global `--json` for stable automation output. Unsafe
publication, rejection, cancellation, principal/worker disabling, capability replacement, and
credential rotation prompt for the exact operation; automation must pass global `--yes`.

## Credentials and profiles

Passwords are read interactively or from owner-only regular files. Passwords and raw tokens are
never accepted as command arguments. Bearer credentials are resolved in this order:

1. `STABBUR_TOKEN` supplied by a protected process environment;
2. `--token-file` / `STABBUR_TOKEN_FILE`, mode 0600 or stricter on Unix;
3. the protected profile created by `stabbur auth login`.

The default profile is stored under the platform configuration directory. Its directory is mode
0700 and the file is created atomically with mode 0600 on Unix. Use `--no-save` for ephemeral
login or `--profile` to select an explicit profile. Prefer token files or a secret manager for
non-interactive services.

First-administrator bootstrap needs no existing profile or bearer token. `stabbur bootstrap` and
`stabbur auth bootstrap` are equivalent; both call the one-time public bootstrap operation. Supply
`--bootstrap-secret-file` and `--password-file` for unattended setup. If either is omitted, the CLI
prompts only when an interactive terminal is available.

An authenticated administrator can reset a human password with
`stabbur auth reset-password USER --password-file FILE`. The longer
`stabbur auth principal reset-password` form remains available. Account lockout recovery is a
local server operation: stop the API and use `stabbur-server admin reset-password`.

## Command map

| Command     | v0.0.1 operations                                                            |
| ----------- | ---------------------------------------------------------------------------- |
| `bootstrap` | one-time unauthenticated first-administrator creation                        |
| `auth`      | bootstrap, login, password change/reset, principals, sessions, tokens, roles |
| `catalog`   | desired-state plan/sync, snapshots, exact lookup, durable catalog scans      |
| `target`    | manual/interval desired policy, updates, triggers, and target run history    |
| `software`  | list/show/create, update name, update installation metadata                  |
| `release`   | list/show, explicit promotion, rejection                                     |
| `variant`   | list/show within a release                                                   |
| `channel`   | list/show, create or advance testing/stable                                  |
| `recipe`    | list/show/create, validated builder revisions, recipe runs                   |
| `run`       | list/create/show, logs, live watch, cancellation                             |
| `artifact`  | metadata, locations, streaming upload, resumable verified download           |
| `storage`   | list/show and non-mutating adapter test                                      |
| `worker`    | list/show, provision, enable/drain, capability ceiling, token rotation       |
| `job`       | list lightweight summaries and show full payload                             |
| `audit`     | cursor-paginated append-only events                                          |
| `resolve`   | select exactly one readable primary installer for a target                   |

Use `stabbur <resource> <operation> --help` for exact fields and workflow examples. Mutable commands
require the current numeric `--revision` shown by `show`; revision `0` creates a channel. Interactive
promotion reads the current revision when omitted. Automation keeps revision `0` as the default,
or can explicitly use `--current-revision`. A concurrent change still rejects the promotion; the
CLI never automatically retries it with a newer revision.

Use `stabbur target trigger NAME --idempotency-key UNIQUE_KEY --watch` to trigger and follow a run.
`--timeout-seconds` bounds the watch, including reconnections. Success exits 0, failure 2,
cancellation 3, and timeout 124. Stopping a watch leaves the server-side build running.
Retryable operations require or accept an idempotency key.

Catalog synchronization is additive and history-preserving: it creates missing software and
recipes, updates managed software metadata with optimistic concurrency, and appends a revision
only when the normalized desired revision differs from the latest one. It never deletes unlisted
resources or schedules a build. See [catalog manifests](docs/catalog.md) and the committed
[schema version 1 example](examples/catalog-v1.json).

Catalog observations are advisory and append-only. `catalog scan request` queues a pinned AutoPkg
repository for a capable outbound worker; `scan list/show/cancel`, `snapshots`, `show-snapshot`,
and `resolve` expose durable status and exact latest-source observations. Nothing discovered is
built until an operator binds a reviewed immutable revision with `target create` and explicitly
triggers it or selects a fixed `--every-seconds` interval.

## Remote workers and AutoPkg

The server controls durable runs, capability matching, leases, retries, result verification, and
candidate publication. Workers initiate outbound HTTPS connections; the server never opens SSH or
a remote shell. Provision a bounded capability ceiling and write the one-time credential directly
to a new owner-only JSON file:

```bash
stabbur-server worker --print-capabilities

stabbur worker provision \
  --name mac-builder-01 \
  --capability runtime.portable \
  --capability builder.fake \
  --capability builder.autopkg \
  --capability os.macos \
  --capability tool.apple-xcode \
  --output-token-file ./mac-builder-01.worker.json
```

Run capability inspection as the final worker account after its tools are installed, and use the
complete reported list as the server-side ceiling. A partial ceiling is rejected rather than
silently hiding newly detected execution capability.

Move that file through a protected channel to the macOS host, then run the server-supplied worker
runtime under the local service supervisor:

```bash
stabbur-server worker \
  --server-url https://stabbur.example.net \
  --token-file /etc/stabbur/mac-builder-01.worker.json \
  --data-dir /var/lib/stabbur-worker
```

The runtime detects and advertises AutoPkg and Apple tooling. The server rejects registration if
that exact advertisement exceeds its configured ceiling; after acceptance, the worker claims
compatible work, heartbeats leases, uploads selected artifacts, and submits builder-neutral
results. See [Remote workers and packagers](docs/worker-operations.md) for recipe control, fan-out,
drain/rotation, service supervision, and client/packager boundaries.

## Build and release targets

Rust 1.88 is the MSRV. CI tests stable, beta, and nightly across Linux x86_64, macOS ARM64, and
Windows x86_64. Release jobs produce static Linux x86_64 and aarch64 archives, a self-contained
macOS ARM64 archive, and a static-CRT Windows x86_64 archive. Every archive has a SHA-256 checksum;
tagged releases must point to a commit that already passed main CI.

Download platform archives and checksums from the
[0.0.1 release](https://github.com/terjekv/stabbur-cli/releases/tag/v0.0.1).
Source builds fetch the exact released public client revision recorded in `Cargo.toml` and
`Cargo.lock`; a sibling checkout is unnecessary. Run the gates in [AGENTS.md](AGENTS.md).
The release workflow repeats CLI and browser acceptance against the immutable server image in
[COMPATIBILITY.md](COMPATIBILITY.md) before publishing the verified platform archives.

## Coordinated operator workflows

The 0.0.1 contract includes catalog schema 2, exact target/revision reconciliation,
software status, release withdrawal, worker draining, and bounded reconnecting run watches.
See the server's [operator workflow guide](../stabbur/docs/operator-workflows.md) and the independent
[management console](../stabbur-frontend/README.md). Schema 1 catalogs remain accepted without targets.
Local cross-repository integration does not replace immutable released-image acceptance.

## Import discovered AutoPkg recipes

Use `stabbur catalog snapshots` to select an immutable worker or repository snapshot. Create a
selection file such as:

```json
[
  {
    "identifier": "com.example.download.App",
    "slug": "app",
    "name": "App",
    "architecture": "aarch64",
    "minimum_macos": "13",
    "version_variable": "version",
    "artifact_variable": "pathname",
    "media_type": "application/octet-stream"
  }
]
```

```sh
stabbur catalog import --snapshot SNAPSHOT_ID --selections selections.json --output imported.json
stabbur catalog plan --file imported.json
stabbur catalog sync --file imported.json
```

Import writes a new file without changing the server. Targets are disabled and manual. Select
`pkg_path` instead of `pathname` for a generated package and review the recipe's artifact
architecture and verification policy. Imports retain exact repository sources, including committed
overrides and their parents. Resolve discovery blockers before importing. Trust is never accepted
automatically. Reusing existing software, recipe or target names may update those resources when
you apply the catalog plan; review the complete plan before syncing.

## Saved batch exports (development server)

Create a named software selection in the console, or use `exports save --file definition.json`.
Follow explicit channels or pin exact releases; no version ordering is inferred. Installation
settings are reused on upgrades. Definitions and history are shared with the console.

```sh
stabbur exports list --all
stabbur exports show staff-macs
stabbur exports plan staff-macs --output reviewed-plan.json
stabbur exports apply --plan-file reviewed-plan.json
stabbur exports download staff-macs --output new-export
stabbur exports profile staff-macs --output managed-macs.mobileconfig
```

Updating a draft requires `exports save --export NAME --revision N --file definition.json`.
Apply requires a saved plan and confirmation (`--yes` for reviewed automation); a stale preview
fails without partial publication. Download creates `new-export/repository` only after verification
and refuses existing destinations. It contains installers, pkginfo, catalogs and provenance, with
no manifests. Merge files into an existing Munki repository, regenerate catalogs and retain its
assignments. Profiles are owner-only files containing an export-only device credential; use
`exports revoke-profiles NAME` to revoke all earlier profiles for that export. `--test-all` requests
installation of every selected application and belongs only on disposable test Macs.

These commands require the coordinated development server. The released 0.0.1 image does not
implement saved exports. The earlier `munki-export` command remains available.
