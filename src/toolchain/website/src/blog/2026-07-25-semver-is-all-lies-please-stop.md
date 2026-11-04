---
layout: page.njk
title: "SemVer Is All Lies. Please Stop"
excerpt: "Semantic versioning records maintainer intent, not compatibility proof. Osprey's package plan proposes exact identity, compatibility epochs, and downstream testing."
description: "Why semantic versioning cannot prove backward compatibility, what breaking-change research shows, and how Osprey's package manager will test dependency updates."
tags: ["blog", "package-management", "semantic-versioning", "breaking-changes", "compatibility", "testing"]
author: "Christian Findlay"
modified: 2026-07-25
readingTime: 9
image: /assets/images/blog/semver-is-all-lies-please-stop.png
imageWebp: /assets/images/blog/semver-is-all-lies-please-stop.webp
imageAlt: "Three translucent release panels appear orderly while a cyan wireframe osprey scan reveals fractures in the dependency network behind them"
---

SemVer cannot prove whether a release contains breaking changes.

Semantic Versioning (SemVer) takes a maintainer's opinion about compatibility, compresses it into three integers, and invites package managers to treat the result as truth. A patch release claims backward-compatible bug fixes. A minor release claims backward-compatible functionality. A major release may contain incompatible public-API changes. In ecosystems that use caret ranges, the resolver may then admit updates based on those claims.

Except the number has not proved any of that. It records what somebody hoped, intended, or believed about a change. Sometimes they are right. Sometimes they have not even identified the public contract that their users depend on.

I do not think maintainers are dishonest. The lie is the certainty. `1.4.7` looks precise enough for a machine to trust, while concealing how little we actually know about whether a particular consumer will survive the upgrade.

## Why semantic versioning cannot prove backward compatibility

[The SemVer specification](https://semver.org/) sounds wonderfully crisp: increment MAJOR for incompatible API changes, MINOR for backward-compatible functionality, and PATCH for backward-compatible bug fixes.

The hard part is hidden inside one word: *compatible*.

Compatible with which consumer? Compiled or recompiled? On which target and toolchain? At the source, binary, ABI, linkage, effect, capability, performance, or behavioral level? Does fixing incorrect behavior break an application that relied on it? Does adding a method break somebody's subclass? Does changing a transitive dependency alter behavior even though your own exported signatures remain untouched?

Compatibility is not an intrinsic property of a diff. It is a relationship among a change, a consumer's usage, its environment, and assumptions the maintainer may never have known existed.

Three integers cannot contain that relationship.

## Developers cannot reliably classify compatibility

This is not merely a complaint from someone who dislikes version syntax.

Jens Dietrich, Kamil Jezek, and Premek Brada gave concrete compatibility puzzles to 414 Java developers in [*What Java Developers Know About Compatibility, And Why This Matters*](https://arxiv.org/abs/1408.2607). Participants answered only **51% of the library-upgrade questions correctly**. Even self-described experts and gurus, and developers with more than ten years of Java experience, managed only about **60%**.

That study did not ask people which SemVer digit they would increment. It tested the more fundamental judgment SemVer requires them to make: will this library change remain source-, binary-, and behaviorally compatible? Developers did much better when recompilation was part of the scenario, which is precisely the problem. Source compatibility is only one of several ways a release can break its users.

The ecosystem evidence reaches the same conclusion from a different direction.

An early Maven study found that [**30.0%** of 80,589 analyzed release pairs contained at least one binary API breaking change](https://doi.org/10.1109/SCAM.2014.30). A later, larger replication addressed limitations in that protocol and found an **83.4%** SemVer-compliance rate across 119,879 statically analyzed upgrades. Even so, [**20.1%** of the 98,591 non-major upgrades contained a detected API breaking change](https://doi.org/10.1007/s10664-021-10052-y). That is a useful statistical signal. It is nowhere near a compatibility guarantee.

The failure becomes more concrete when we look at clients instead of signatures. Researchers running real npm client test suites found that dependency-induced breaks during non-major updates affected [45 of 384 sampled dependent packages (**11.7%**) and 450 of 3,230 tested releases (**13.9%**)](https://doi.org/10.1145/3576037). Minor and patch labels did not prevent those failures. Of 64 manually analyzed cases, 37 (**57.8%**) originated in an indirect provider, so the package that broke the application was often not one its developer chose directly.

SemVer is better than random numbering. That is an extraordinarily low bar. It remains a maintainer assertion presented in a form that encourages automated trust.

## The only honest response is extreme testing

A version number is an assertion. A downstream test is evidence.

One failing unchanged client test can prove a specific observed incompatibility: this consumer, under these inputs and this environment, no longer behaves as expected. A green suite proves something narrower: the behavior covered by those tests survived. It cannot prove that every untested consumer, target, call path, or assumption is safe.

That distinction matters because ordinary testing is nowhere near enough. Hejderup and Gousios studied 521 well-tested Java projects and found median test coverage of **58%** for direct-dependency calls and **21%** for call paths to transitive dependencies. In a 262-project subset, they generated 1,122,420 artificial updates with simple faults; tests detected a mean [**47%** of direct-dependency faults and **35%** of transitive-dependency faults](https://doi.org/10.1016/j.jss.2021.111097).

So by **extreme testing**, I do not mean “run the package's unit tests and hope harder.” I mean collecting independent evidence from every practical angle:

- diff the public types, values, effects, capabilities, contracts, and ABI;
- compile and type-check real consumers against the candidate release;
- run unit, integration, property, fuzz, and cross-target tests;
- mutation-test the tests themselves so weak assertions do not look impressive;
- substitute the candidate into affected clients while keeping the rest of their dependency graph pinned;
- record failures, timeouts, exclusions, sample sizes, tool versions, and confidence instead of collapsing everything into a green tick.

Research on [using dependent projects' tests](https://doi.org/10.1145/3379597.3387476) detected six of ten selected breakage-inducing npm versions. [Cross-project compatibility testing](https://doi.org/10.1145/3377811.3380436) also found JDK incompatibilities that the JDK regression suite missed. Client tests therefore contribute real usage scenarios and expected behavior that library-owned tests may not cover.

Extreme testing is not a proof that nothing broke. It is the only honest way to accumulate evidence about what did.

## Flutter runs other people's tests

Flutter offers one of the best real-world examples of this attitude.

Its [Customer Test Registry](https://github.com/flutter/tests/blob/main/README.md) contains references to tests for applications and libraries that are typically maintained by people outside the Flutter team. Flutter runs those tests with every commit to give the team visibility into how Flutter changes affect real-world developers. The public registry is only part of the picture: Flutter's breaking-change process says Google allows the team to run [several tens of thousands of proprietary tests on each commit](https://github.com/flutter/flutter/blob/main/docs/contributing/Tree-hygiene.md#handling-breaking-changes).

The workflow is refreshingly empirical. Implement the proposed Flutter change, then run the existing contributed tests **without changing them first**. If one requires changes, Flutter classifies the proposal as a breaking change.

This does not mean Flutter forbids breaking changes. Its [compatibility policy](https://docs.flutter.dev/release/compatibility-policy) says Flutter works with the developers of affected tests to judge whether a change is sufficiently valuable and to provide fixes so those tests continue to pass. For a breaking change governed by that process and judged valuable enough, the team stages the change, migrates participating clients, publishes a migration guide, announces the change, and includes it in the release notes.

The tests are an instrument panel, not a constitutional ban on change. They turn an invisible cost into information that people can rationalize. The team can preserve compatibility, redesign the change, or accept the break with concrete evidence about which contributed tests and participating clients require migration and how the change will be communicated.

[Flutter's dependency-management documentation](https://docs.flutter.dev/packages-and-plugins/dependency-management) states that Dart packages use Semantic Versioning (SemVer). I am pointing to Flutter's compatibility-detection practice, not claiming that Flutter has rejected version numbers. That practice illustrates my argument: Flutter's compatibility policy is based on whether submitted tests fail, so the team runs contributed application and library tests rather than inferring compatibility from a version number.

## Osprey's package-manager plan separates identity, order, and compatibility

This is why the [Osprey package-manager plan](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0020-package-manager.md) and [package specification](/spec/0029-packagemanagement/) reject SemVer fields and version ranges. The design is specified, not implemented yet, and it gives the three jobs hidden inside a semantic version to three different mechanisms:

| Concern | Osprey mechanism | What it actually says |
| --- | --- | --- |
| Identity | Immutable release digest | This identifies the exact package key, epoch, source, manifest, and build plan. |
| Order | Registry-assigned ordinal | This is the release's package-wide sequence position. |
| Compatibility | Registry-checked epoch | These releases claim one contract lineage, subject to registry checks. |

The epoch keeps the one defensible idea behind a major version: sometimes a package deliberately opens a new incompatible lineage. It discards the false precision of author-selected minor and patch numbers.

A normal dependency names a package, an epoch, and a minimum release ordinal. The lock records the exact digest. An ordinary update remains within the current epoch. A breaking update crosses epochs explicitly and produces a migration report; the resolver never wanders across that boundary because a range expression happened to admit it.

Publication still begins with a maintainer's claim. It just does not end there. The planned registry checks public API, ABI, types, effects, capabilities, and machine-readable contracts. Candidate updates are compiled, checked against affected clients, and subjected to unit, integration, property, fuzz, and mutation tests. Results retain denominators and confidence. The plan explicitly forbids describing a clean assessment as proof that a release is safe.

If evidence reveals a break, the right outcome is not automatically “stop.” The right outcome is **know**. Reject an accidental break. Open a new epoch for an intentional one. Stage it, explain it, generate the migration information, and let consumers decide with facts in front of them.

## Please stop outsourcing judgment to three integers

Breaking changes are sometimes correct. A bad API should not be immortal. A security repair may require disruption. A behavior that users accidentally relied on may be too damaging to preserve.

The job of dependency management is not to pretend those decisions disappeared because somebody typed `PATCH` instead of `MAJOR`. Its job is to identify exact inputs, expose observed consequences, preserve reproducibility, and make compatibility decisions explicit.

A digest can verify the exact release envelope. An ordinal can identify its package-wide sequence position. An epoch can record an explicit compatibility boundary. Tests, contracts, analysis, and real client runs can tell me what we actually observed.

`1.4.7` cannot tell me any of that.

SemVer offers a comforting story in which maintainers can know every dependency their users have, every behavior they rely on, and every way a change can break them. The research says otherwise. The real world says otherwise. Flutter's testing process says otherwise.

Measure the break. Decide whether it is justified. Communicate it clearly.

Please stop calling the number proof.
