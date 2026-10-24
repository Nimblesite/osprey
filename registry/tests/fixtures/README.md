# Package-system fixtures

Declarative inputs for the suites in `registry/tests/src`. Every file is data; nothing here executes. Each JSON file names the spec section it pins in a `spec` field.

| File | Pins |
| --- | --- |
| `names.json` | `[PACKAGE-NAMES]` accepted keys and each rejection's reason code. |
| `release-ids.json` | `[PACKAGE-IDENTITY]` display forms, including malformed digests, epochs and ordinals. |
| `paths.json` | `[PACKAGE-SOURCE-CANONICAL]` path grammar, case-fold collisions, excluded control paths, forbidden entry kinds, the LFS pointer shape and submission bounds. |
| `source/*.json` | Whole trees with each file's SHA-256, the exact envelope bytes and `SourceDigest`; `minimal-tree.json` also carries a digest-substitution variant. |
| `manifests/ledger/` | The spec 0029 example manifest, its RFC 8785 canonical JSON and `H("osprey.manifest.v1", …)`. |
| `digests.json` | `H(d, x)` over small payloads. |
| `canonical/` | One value per serialized model type with its RFC 8785 bytes and, where spec 0031 names a domain, `H(d, x)`; `relations.json` binds them: the acyclic selection → build plan → lock staging, every self-omitting digest (`proofDigest`, `generationDigest`, `journalDigest`) and every envelope digest. `index.json` lists the folder. |
| `schema/` | `[PACKAGE-TYPE-INVARIANTS]` JSON encoding: values every type accepts, and rejections with their code and JSON pointer. |
| `time/datetimes.json` | Exact UTC millisecond timestamps, valid and invalid. |
| `order/lists.json` | Every list order and duplicate rule of spec 0031, as sorted / swapped / same-key triples. |
| `malicious/` | Unicode confusables in names and paths; PNG structure and decompression-bomb vectors. |
| `graphs/` | Bounded dependency graphs with the one expected lock or explanation, checked by an exhaustive oracle. |
| `vectors/` | Primary-specification vectors: FIPS 180-4 SHA-256, digest agility, RFC 8785, DSSE PAE and WebAuthn bytes, RFC 9162 Merkle proofs, TUF 1.0 client workflow and compromised mirrors. |
| `transitions/release-status.json` | Release-status chains and every invalid move, with independently computed `previousStatus` digests. |
| `folder/refusals.json` | One folder-registry walk with every refusal the loader reports. |
| `schema-map.json` | Which fixture values are model objects and of which type; `registry/fixture-check` validates every one before any suite runs. |

Goldens were computed once with an independent implementation (Python `hashlib` and `json`, which agrees with RFC 8785 for ASCII keys and integer-only values) and are asserted byte for byte by Osprey code that never saw that implementation. Change a fixture only together with the spec change that justifies it.
