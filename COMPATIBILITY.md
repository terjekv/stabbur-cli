# Server compatibility

CLI 0.0.1 targets released Stabbur server 0.0.1 through `stabbur_client = 0.0.1`.
The client dependency uses the immutable source revision corresponding to its
[0.0.1 release](https://github.com/terjekv/stabbur-client-rust/releases/tag/v0.0.1):

```text
802b60e653b75b3694b0ed4607a6f61a139ba093
```

The complete 75-operation public contract belongs to that client. Every CLI API operation uses
its blocking interface; the CLI constructs no Stabbur HTTP requests.

The [server publication run](https://github.com/terjekv/stabbur/actions/runs/36231611736) passed client,
CLI and console acceptance against this immutable Linux amd64 image:

```text
ghcr.io/terjekv/stabbur-server@sha256:41aba0e2051de7d74df09a6e57487a4e1755706d587aca54603167953fd0748d
```

[Recorded evidence](evidence/server-0.0.1.json) identifies the exact source revisions, main CI runs,
OpenAPI hash and separate disposable macOS installation/recovery acceptance.
The CLI release workflow checks the released client tag, repeats CLI and browser workflows against
this same image, and publishes only the checksummed Linux x86_64/aarch64, macOS ARM64 and Windows
x86_64 archives from successful main CI. The
[0.0.1 release](https://github.com/terjekv/stabbur-cli/releases/tag/v0.0.1) includes
`release-evidence.json` identifying the final CLI source, CI run and live acceptance run.

Rust 1.88 remains the MSRV. Stable, beta and nightly platform tests, release linkage checks,
package checksums, and dependency/advisory policy must pass before publication.

## Unreleased saved-export extension

The development CLI pins export client `0634d8c177aa90c1bd86056a75fc67d2ae282ce5` and uses its 86-operation contract.
`exports` commands require the matching development server. The immutable 0.0.1 image and release
evidence above describe the earlier 75-operation release and do not claim saved-export support.
