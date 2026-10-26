# Security policy

## Reporting a vulnerability

Report vulnerabilities privately through GitHub: open the repository's
**Security** tab and choose **Report a vulnerability**. Do not open a public
issue for a security problem. A report is acknowledged within seven days and
the aim is a fix or a documented mitigation within ninety days of
acknowledgement, with credit to the reporter unless they ask otherwise.

## Scope

- The Osprey compiler and runtime, including the HTTP, process and file
  builtins.
- The package system under `packages/manager/`: the folder-registry server, the
  package frontend (`fetch`, `lock`, `verify`, `build`), the manifest and lock
  parsers and the resolver. Its threat model and the gates a report can test
  against are in
  [spec 0029](docs/specs/ecosystem/0029-PackageManagement.md), sections
  `[PACKAGE-THREATS]` and `[PACKAGE-HARDENING]`.
- The VS Code extension and the website build.

A finding that a documented gate answers differently from its stated code is
a vulnerability report, not a feature request.

## Out of scope

- Denial of service against a folder registry run without the reverse proxy
  that `[PACKAGE-HARDENING]` S5 requires beyond loopback.
- Vulnerabilities in third-party dependencies of a user's own project.
