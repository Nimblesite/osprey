# Package Domain Model

> **ALL OSPREY, and A WORKING API THAT SERVES WORKING PACKAGES COMES FIRST.**
> Every executable part of the package system is Osprey, with no exceptions
> (`[PACKAGE-IMPLEMENTATION-LANGUAGE]`). The first deliverable is the
> database-free folder registry of `[PACKAGE-FOLDER-REGISTRY]` in
> [spec 0029](0029-PackageManagement.md); everything in this document layers
> onto that running service.

**Status:** normative target. This is the typed data contract for
[Package Management](0029-PackageManagement.md) and
[Registry Trust and Discovery](0030-PackageRegistryTrustAndDiscovery.md), with
score semantics fixed by [Evidence and Fair Discovery](0032-PackageEvidenceAndDiscovery.md).

The model uses current
[typeDiagram language syntax](https://typediagram.dev/docs/language-reference.html).
It is intentionally expressed with `type`, `union`, and `alias` so typeDiagram
can generate the Osprey model when that output target lands. Field removal,
meaning change, or union narrowing is a schema-breaking change; additive fields
require a lock/API format bump unless they have an explicit compatible default.

Executable model generation, validation, canonical serialization, and conformance tooling MUST be implemented in Osprey under [the implementation-language contract](0029-PackageManagement.md#implementation-language-package-implementation-language). Generated package-domain source MUST be Osprey; a handwritten or generated implementation in another language is not permitted. Until Osprey model generation is available, Osprey code consumes serialized conformance fixtures directly, without a duplicate handwritten domain model. The typeDiagram schema and serialized fixtures remain declarative data.

## Reading the model

An organization owns a package. One administrator chooses its publishers. A publishing grant names one package, one membership, one permission and the two authority epochs that make the grant current. See [ownership and delegation](0045-OrganizationPackageGovernance.md).

A release follows its repository binding, intent, challenge and signed authorization. A client verifies its source digest, signed status, transparency log and witness receipts before activating it. Build plans and locks record the exact inputs needed to reproduce it. The final section describes assessment and discovery. See the [model invariants](0031-PackageDomainInvariants.md).

[Read the canonical typeDiagram](https://nimblesite.github.io/osprey-packages/).

<!-- registry-model -->
