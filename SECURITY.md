# Security policy

Report vulnerabilities privately. Do not include tokens, passwords, profile contents, worker
credential JSON, artifact authorization, or a working exploit in public issues, fixtures, command
transcripts, or logs.

## CLI security properties

- Passwords are interactive or read from owner-only regular files; there is no password option or
  positional value.
- Bearer tokens come from a protected environment, owner-only file, or owner-only profile and are
  never printed.
- New API-token and worker credential files use exclusive creation and mode 0600 on Unix, so an
  existing destination is never overwritten.
- One-time worker files contain the server-issued worker UUIDv7 and token expected by the outbound
  runtime; secrets are never placed in process arguments.
- Publication, rejection, cancellation, disabling, capability replacement, and credential rotation
  require interactive confirmation or explicit global `--yes`.
- Mutable commands require an exact revision and rely on strong ETags to reject stale automation.
- JSON input files are regular files capped at 2 MiB. Sensitive run parameter names are rejected
  by the server.
- Artifact transfers are bounded; completed downloads are SHA-256 verified before success.
- Human table fields collapse control whitespace to prevent terminal layout injection.

Use HTTPS for every non-loopback server, restrict profile and artifact-output permissions to the
intended account, and use a process supervisor plus an unprivileged account for remote workers.
