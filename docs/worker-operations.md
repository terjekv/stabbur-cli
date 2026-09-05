# Remote workers, clients, and packagers

## Responsibility boundary

The control plane owns desired work and durable truth. A remote worker owns only execution of a
leased attempt on its local host.

```text
operator creates run
        |
server stores job and capability requirements
        |
outbound worker registers detected capabilities and claims a compatible lease
        |
worker heartbeats, runs the isolated builder, and uploads selected artifacts
        |
server rehashes, verifies, records provenance, and decides candidate publication
```

This is command-and-control through a narrow job protocol, not arbitrary remote execution. The
server does not SSH to workers, accept shell snippets, or expose the internal lease protocol via
the public Rust client or CLI.

The same queue carries builder-neutral recipe-catalog scan jobs. `stabbur catalog scan request`
records an exact pinned source; a capable outbound worker claims it, generates a bounded neutral
manifest, and atomically publishes an immutable observation or safe typed failure. AutoCfg can
call the public operation or consume the neutral generator output without exposing its cache
layout to Stabbur.

## Provision and supervise a worker

Create a server-side identity with the smallest justified capability ceiling:

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

Run the inspection on the final worker account after installing its tools. Its JSON is the exact
advertisement; configure the complete list as the ceiling. Registration returns
`worker_capability_escalation` if detection finds a capability outside that ceiling. This makes a
local tool change an explicit server-side authorization decision.

The output is server-issued JSON containing `worker_id` and `token`, created with exclusive file
creation and mode 0600 on Unix. Copy it to the target using a protected channel. Do not paste it
into a shell history, issue tracker, or service-unit argument.

Start the outbound runtime:

```bash
stabbur-server worker \
  --server-url https://stabbur.example.net \
  --token-file /etc/stabbur/mac-builder-01.worker.json \
  --data-dir /var/lib/stabbur-worker \
  --autopkg-program /Library/AutoPkg/autopkg
```

Run it as an unprivileged local account under launchd or another service supervisor with restart
on failure. The data and credential directories should be accessible only to that account. Remote
servers require HTTPS; plain HTTP is accepted only for loopback testing.

`--autopkg-program` is optional for a standard installation. When supplied, it is a worker-local
absolute path, never a server command; the runtime rejects non-executable or peer-writable targets.
Use the same option with `worker --print-capabilities` before setting the server-side ceiling.

At startup the runtime detects local tools. A macOS host with usable AutoPkg advertises
`builder.autopkg` and Apple-tool capabilities; Linux and other hosts advertise only their portable
capabilities. The server rejects an advertisement outside the provisioned ceiling. Adding tools
locally cannot escalate server authorization or become eligible for jobs until an administrator
deliberately expands the ceiling.

## Define and run AutoPkg work

An immutable builder-neutral revision selects `autopkg`, pins every Git source to a full commit,
and declares report selectors, required capabilities, and required verification checks. Create the
reviewed JSON revision:

```bash
stabbur recipe create --name firefox --idempotency-key recipe-firefox
stabbur recipe create-revision firefox \
  --file firefox-autopkg-revision.json \
  --idempotency-key firefox-revision-1
```

The file uses the public builder-neutral envelope. Its `definition` contains the complete
AutoPkg-specific source, entrypoint, input, and output-selector fields. A portable deterministic
smoke revision instead uses:

```json
{
  "builder": "fake",
  "definition": {},
  "required_capabilities": []
}
```

Queue work against the returned revision UUIDv7:

```bash
stabbur run create \
  --software firefox \
  --recipe-revision REVISION_UUID \
  --parameters non-secret-parameters.json \
  --idempotency-key firefox-run-2026-08-26

stabbur run watch RUN_UUID
stabbur run show RUN_UUID
```

For reusable desired policy, create a target instead of scripting raw run creation:

```bash
stabbur target create --name firefox-daily --software firefox \
  --recipe-revision REVISION_UUID --every-seconds 86400
stabbur target runs firefox-daily
```

The runtime invokes AutoPkg directly without a shell in isolated home/cache/work directories,
materializes only pinned commits, captures bounded processor receipts, and streams stdout/stderr as
ordered logs. Persisted report paths are attempt-relative and private roots are redacted from logs.
Pinned base recipes and AutoPkg-verified overrides have distinct recorded trust methods. The worker
uploads artifacts before submitting the result. The server rehashes every upload and publishes
candidate only when recipe trust, required verification, primary-installer cardinality, and a
readable present primary location all pass.

## Fan-out, drain, and rotation

Multiple workers may connect concurrently. The server matches required capabilities and uses
leases with heartbeats; it does not push to a chosen hostname. Expired attempts are eligible for
retry, and idempotent transactional completion prevents duplicate releases.

Inspect and change a worker with its current revision:

```bash
stabbur worker show WORKER_UUID
stabbur worker set-capabilities WORKER_UUID \
  --revision 3 \
  --capability runtime.portable \
  --capability builder.fake \
  --capability builder.autopkg \
  --capability os.macos \
  --capability tool.apple-xcode \
  --yes
stabbur worker disable WORKER_UUID --revision 4 --yes
```

Disabling, narrowing capabilities, or rotating a credential invalidates an active attempt. Drain
before maintenance when possible. Rotation writes a replacement owner-only JSON file:

```bash
stabbur worker rotate-token WORKER_UUID \
  --revision 5 \
  --output-token-file ./replacement.worker.json \
  --yes
```

Install the replacement atomically on the worker and restart it. The old credential stops working
immediately.

## Client and packager behavior

Interactive operators use this CLI; Rust automation uses `stabbur_client`. Installers and
packagers are not worker runtimes and do not receive jobs. They query `resolve` with channel,
platform, architecture, and optional numeric macOS version, then download the returned immutable
digest with range/resume support and verify SHA-256 locally. Installation and detection metadata is
declarative input for those clients; it is never executed by the control plane.

This separation lets the server fan out trusted build responsibilities while keeping installation
policy explicit and artifact identity stable across storage or database backends.
