# Server compatibility

The CLI, public client, and server are independently versioned. CLI 0.1.0 pins
`stabbur_client = 0.1.0` exactly; that client owns the authoritative Stabbur 0.1 contract snapshot.

| CLI   | `stabbur_client` | Server target | Evidence                                                                                                                                                  | Status            |
| ----- | ---------------- | ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------- |
| 0.1.0 | 0.1.0            | 0.1.0         | 75-operation client contract, command/parser and mock gateway tests, plus local cross-repository integration recorded on 2026-09-05                       | Release candidate |

Publishing still requires the exact immutable server image digest to be recorded in the client
compatibility matrix and the CLI live workflow to pass against that same digest. A local binary or
moving image tag is not release evidence.
