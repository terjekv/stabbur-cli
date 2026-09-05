# Desired-state catalog manifests

Catalog schema version 1 manages software metadata, recipe metadata, and the desired latest
immutable revision for each recipe. It deliberately does not schedule builds; durable build
targets are managed explicitly with `stabbur target`.

Start with [the version 1 example](../examples/catalog-v1.json), then inspect the plan:

```bash
stabbur --json catalog plan --file catalog.json
stabbur --json catalog sync --file catalog.json
```

`plan` validates and canonicalizes the complete manifest before reading the server. It never
mutates server state. `sync` computes the same plan and applies its ordered actions through the
ordinary public software and recipe APIs. Repeating `sync` after convergence returns an empty
`applied` list.

The manifest is a reviewed allowlist, not a mirror of an AutoPkg repository. Unlisted software and
recipes are left untouched. Removing an entry never deletes history. Within a managed software
entry, omission of `installation` leaves existing installation metadata unmanaged; supplying it
reconciles an exact `install` and `detection` object.

Recipe definitions use the same builder-neutral envelope as `recipe create-revision`. The client
normalizes mandatory capabilities before comparison: `fake` adds `builder.fake` and
`runtime.portable`; `autopkg` adds `builder.autopkg` and `os.macos`. A revision is appended only
when the latest server revision differs after normalization. AutoPkg definitions still require
pinned full Git commits and reviewed selectors as described in
[worker operations](worker-operations.md).

Synchronization is forward-only. Display-name and installation changes use optimistic
concurrency, recipe revisions remain immutable, and concurrent reconcilers may produce a stale
revision error. Run a single reconciler per catalog and rerun after a transport or concurrency
failure; the next plan starts from durable server state.

## Observe upstream catalogs and select work

Discovery is a separate advisory workflow. Queue one exact pinned AutoPkg source with `stabbur
catalog scan request`, inspect durable state with `scan list/show`, inspect immutable observations
with `snapshots/show-snapshot`, and use `catalog resolve IDENTIFIER` to answer whether an exact
recipe exists in each source's latest observation. Scans never mutate the desired-state manifest.

Use `target create` to bind reviewed software and an immutable recipe revision to manual or
fixed-interval policy. `target trigger` creates an idempotent run; `target runs` shows provenance
from that desired target. Updating or disabling a target requires its current numeric revision,
and disabling also requires global `--yes` for automation.
