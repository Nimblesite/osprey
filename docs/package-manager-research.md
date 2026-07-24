# Osprey Package Manager — Research Corpus

**Evidence cutoff:** 24 July 2026  
**Purpose:** academic and standards basis for Osprey package-management design  
**Scope:** versioning, resolution, reproducibility, supply-chain security, maintenance evidence, AI-assisted scanning, and fair discovery

This 66-source record contains 53 peer-reviewed publications, ten final official
standards, specifications, guidance, or reference documents, one official public-comment
draft, and two explicitly marked preprints. Quotations are short source excerpts; each **Osprey consequence** is Osprey's inference, not the source's prescription.

## Decisions supported by the corpus

| Evidence synthesis | Design consequence |
|---|---|
| Version labels are useful communication but unreliable compatibility proofs. | Identify releases by immutable digest and registry ordinal; represent compatibility as a separately checked contract epoch, not SemVer. |
| Dependency solving is a formal constraint-and-optimization problem. | Use a complete solver with explicit hard constraints, lexicographic objectives, deterministic tie-breaking, and explanations. |
| Exact direct pins do not freeze a transitive graph; tags and remote registries can change. | Commit a complete transitive lock with digests, provenance, policy version, and catalog snapshot; verify locally. |
| Tests, coverage, static analysis, and AI each have material blind spots. | Combine independent evidence, publish confidence and raw measurements, and never let an AI verdict create cryptographic trust. |
| Repository, publisher, forge, builder, scanner, and mirror compromises are distinct. | Layer source collection, hermetic builds, TUF-style update metadata, attestations, immutable storage, and witnessed transparency. |
| Popularity feedback is self-reinforcing and manipulable. | Keep stars, downloads, dependents, and paid promotion out of default ranking; reserve measured exposure for vetted, relevant, underexposed packages. |

## 1. Compatibility, dependency solving, and reproducibility

### <a id="r01"></a>R01 — Semantic versioning versus breaking changes

Raemaekers, van Deursen, and Visser, SCAM 2014. **Peer-reviewed.** [DOI 10.1109/SCAM.2014.30](https://doi.org/10.1109/SCAM.2014.30)
> “Around one third of all releases introduce at least one breaking change.”

**Osprey consequence:** a publisher-selected number is not proof of compatibility; the registry computes and records compatibility evidence.

### <a id="r02"></a>R02 — Breaking Bad? Semantic Versioning and Impact of Breaking Changes in Maven Central

Ochoa et al., *Empirical Software Engineering* 27, 2022. **Peer-reviewed.** [DOI 10.1007/s10664-021-10052-y](https://doi.org/10.1007/s10664-021-10052-y)
> “83.4% of these upgrades do comply with semantic versioning.”

**Osprey consequence:** compatibility labels retain informational value, but the non-compliant tail is too large for a security boundary.

### <a id="r03"></a>R03 — A Large Scale Analysis of Semantic Versioning in NPM

Pinckney et al., MSR 2023. **Peer-reviewed.** [DOI 10.1109/MSR59073.2023.00073](https://doi.org/10.1109/MSR59073.2023.00073)
> “Critical updates such as security patches can flow quite rapidly ... in the majority of cases (90.09%).”

**Osprey consequence:** compatible security updates should flow automatically, but only after resolution and verification against the consumer's full graph.

### <a id="r04"></a>R04 — I Depended on You and You Broke Me

Venturini et al., *ACM Transactions on Software Engineering and Methodology* 32, 2023. **Peer-reviewed.** [DOI 10.1145/3576037](https://doi.org/10.1145/3576037)
> “Around 12% of the dependent packages and 14% of their releases were impacted.”

**Osprey consequence:** non-major-looking updates still break clients; activation requires client-impact checks, not label trust.

### <a id="r05"></a>R05 — Putting the Semantics into Semantic Versioning

Lam, Dietrich, and Pearce, Onward! 2020. **Peer-reviewed.** [DOI 10.1145/3426428.3426922](https://doi.org/10.1145/3426428.3426922)
> “Contracts ... are a promising input to semantic versioning calculators.”

**Osprey consequence:** compatibility epochs are backed by type, effect, API, ABI, and behavioral-contract differencing.

### <a id="r06"></a>R06 — Has My Release Disobeyed Semantic Versioning?

Zhang et al., ASE 2022. **Peer-reviewed.** [DOI 10.1145/3551349.3556956](https://doi.org/10.1145/3551349.3556956)
> “Sembid achieved 90.26% recall and 81.29% precision.”

**Osprey consequence:** semantic differencing is a valuable gate and evidence item, but its error rate prevents it from being the sole arbiter.

### <a id="r07"></a>R07 — How Java APIs Break — An Empirical Study

Jezek, Dietrich, and Brada, *Information and Software Technology* 65, 2015. **Peer-reviewed.** [DOI 10.1016/j.infsof.2015.02.014](https://doi.org/10.1016/j.infsof.2015.02.014)
> “API instability is common and causes problems for programs using these APIs.”

**Osprey consequence:** source, binary, linkage, and behavioral compatibility are separate recorded dimensions.

### <a id="r08"></a>R08 — Can We Trust Tests to Automate Dependency Updates?

Hejderup and Gousios, *Journal of Systems and Software* 183, 2022. **Peer-reviewed.** [DOI 10.1016/j.jss.2021.111097](https://doi.org/10.1016/j.jss.2021.111097)
> “Tests can only detect 47% of direct and 35% of indirect artificial faults on average.”

**Osprey consequence:** passing tests never proves compatibility; update validation combines static impact analysis, contracts, compilation, and tests.

### <a id="r09"></a>R09 — UpCy: Safely Updating Outdated Dependencies

Dann, Hermann, and Bodden, ICSE 2023. **Peer-reviewed.** [DOI 10.1109/ICSE48619.2023.00031](https://doi.org/10.1109/ICSE48619.2023.00031)
> “70.1% of the generated updates have zero incompatibilities.”

**Osprey consequence:** the updater searches coordinated graph changes instead of naively bumping one dependency at a time.

### <a id="r10"></a>R10 — Dependency Solving Is Still Hard, but We Are Getting Better at It

Abate et al., SANER 2020. **Peer-reviewed.** [DOI 10.1109/SANER48275.2020.9054837](https://doi.org/10.1109/SANER48275.2020.9054837)
> “Dependency solving is a hard (NP-complete) problem in all non-trivial component models.”

**Osprey consequence:** resolution is a distinct formal subsystem; a greedy fallback is non-conforming.

### <a id="r11"></a>R11 — Flexible and Optimal Dependency Management via Max-SMT

Pinckney et al., ICSE 2023. **Peer-reviewed.** [DOI 10.1109/ICSE48619.2023.00124](https://doi.org/10.1109/ICSE48619.2023.00124)
> “PacSolve ... allows for customizable constraints and optimization goals.”

**Osprey consequence:** hard safety constraints and ordered objectives live in a package-neutral solver model, not scattered resolver heuristics.

### <a id="r12"></a>R12 — OPIUM: Optimal Package Install/Uninstall Manager

Tucker et al., ICSE 2007. **Peer-reviewed.** [DOI 10.1109/ICSE.2007.59](https://doi.org/10.1109/ICSE.2007.59)
> “If there is a solution, Opium is guaranteed to find it.”

**Osprey consequence:** the resolver must be complete for its declared model and must report unsatisfiable constraints with a derivation.

### <a id="r13"></a>R13 — A Modular Package Manager Architecture

Abate et al., *Information and Software Technology* 55, 2013. **Peer-reviewed.** [DOI 10.1016/j.infsof.2012.09.002](https://doi.org/10.1016/j.infsof.2012.09.002)
> “Package managers should be complete, that is find a solution whenever there exists one.”

**Osprey consequence:** catalog, constraint IR, solver, policy, fetcher, verifier, and installer have explicit versioned interfaces.

### <a id="r14"></a>R14 — The Design Space of Lockfiles Across Package Managers

Gamage et al., *Empirical Software Engineering* 31, 2026. **Peer-reviewed.** [DOI 10.1007/s10664-025-10789-w](https://doi.org/10.1007/s10664-025-10789-w)
> “Lockfiles are used ... to verify the integrity of resolved packages; and to support build reproducibility.”

**Osprey consequence:** every application lock records the complete transitive solution, content digests, provenance, and resolution-policy identity.

### <a id="r15"></a>R15 — Pinning Is Futile

He, Vasilescu, and Kästner, *Proceedings of the ACM on Software Engineering*, FSE 2025. **Peer-reviewed.** [DOI 10.1145/3715728](https://doi.org/10.1145/3715728)
> “Pinning direct dependencies ... increases the cost of maintaining vulnerable and outdated dependencies.”

**Osprey consequence:** source manifests express compatibility; a separately generated full-closure lock supplies reproducibility and coordinated updates.

### <a id="r16"></a>R16 — A Comprehensive Study of Bloated Dependencies in the Maven Ecosystem

Soto-Valero et al., *Empirical Software Engineering* 26, 2021. **Peer-reviewed.** [DOI 10.1007/s10664-020-09914-8](https://doi.org/10.1007/s10664-020-09914-8)
> “75.1% of the analyzed dependency relationships are bloated.”

**Osprey consequence:** closure size and unused reachability are resolver objectives and visible evidence; strict package-key uniqueness prevents duplicate-version bloat.

### <a id="r17"></a>R17 — Nix: A Safe and Policy-Free System for Software Deployment

Dolstra, de Jonge, and Visser, LISA 2004. **Peer-reviewed.** [USENIX paper](https://www.usenix.org/legacy/events/lisa04/tech/full_papers/dolstra/dolstra.pdf)
> “A component is never overwritten after it has been built.”

**Osprey consequence:** verified package trees enter an immutable, content-addressed store and installations reference store identities.

### <a id="r18"></a>R18 — Reproducible Builds: Increasing the Integrity of Software Supply Chains

Lamb and Zacchiroli, *IEEE Software* 39, 2022. **Peer-reviewed.** [DOI 10.1109/MS.2021.3073045](https://doi.org/10.1109/MS.2021.3073045)
> “Trusting the code is not the same as trusting the executable.”

**Osprey consequence:** registry builders are hermetic and independently reproduced; provenance alone does not establish source-to-output equivalence.

### <a id="r19"></a>R19 — Mutating the “Immutable”: A Large-Scale Study of Git Tag Alterations

Rapaport et al., 2026. **Preprint — not peer-reviewed.** [arXiv:2606.31354](https://arxiv.org/abs/2606.31354)
> “32 packages reference tags altered in our dataset, with 7 exhibiting confirmed build errors.”

**Osprey consequence:** tags and branches are discovery metadata only; source intake pins a commit and the release identity is the canonical source digest.

### <a id="r64"></a>R64 — Package Managers à la Carte: A Formal Model of Dependency Resolution

Gibb et al., ICFP 2026. **Peer-reviewed, accepted.** [Official conference record](https://icfp26.sigplan.org/details/icfp-2026-icfp-papers/27/Package-Managers-la-Carte)
> “Root inclusion, dependency closure, and version uniqueness.”

**Osprey consequence:** every root and transitive edge resolves in one graph with exactly one selected source per package key; external system inputs become explicit and pinned.

## 2. Supply-chain attacks, provenance, and transparency

### <a id="r20"></a>R20 — A Look in the Mirror: Attacks on Package Managers

Cappos et al., CCS 2008. **Peer-reviewed.** [DOI 10.1145/1455770.1455841](https://doi.org/10.1145/1455770.1455841)
> “All of these package managers have vulnerabilities ... exploited by a man-in-the-middle or a malicious mirror.”

**Osprey consequence:** clients authenticate signed metadata and target digests; TLS and mirror reputation are insufficient.

### <a id="r21"></a>R21 — Survivable Key Compromise in Software Update Systems

Samuel et al., CCS 2010. **Peer-reviewed.** [DOI 10.1145/1866307.1866315](https://doi.org/10.1145/1866307.1866315)
> “TUF ... uses multiple-signature trust and role separation.”

**Osprey consequence:** offline threshold root keys, delegated targets, short-lived online roles, expiration, and key rotation are mandatory.

### <a id="r22"></a>R22 — in-toto: Providing Farm-to-Table Guarantees for Bits and Bytes

Torres-Arias et al., USENIX Security 2019. **Peer-reviewed.** [USENIX publication](https://www.usenix.org/conference/usenixsecurity19/presentation/torres-arias)
> “A framework that cryptographically ensures the integrity of the software supply chain.”

**Osprey consequence:** each intake, scan, build, test, and publication step emits a signed, digest-bound attestation.

### <a id="r23"></a>R23 — Sigstore: Software Signing for Everybody

Newman, Meyers, and Torres-Arias, CCS 2022. **Peer-reviewed.** [DOI 10.1145/3548606.3560596](https://doi.org/10.1145/3548606.3560596)
> “It enables developers to use ephemeral keys to sign their artifacts.”

**Osprey consequence:** publishing uses short-lived OIDC-bound credentials and transparency; authenticated identity is still not a safety verdict.

### <a id="r24"></a>R24 — SoK: Taxonomy of Attacks on Open-Source Software Supply Chains

Ladisa et al., IEEE Symposium on Security and Privacy 2023. **Peer-reviewed.** [DOI 10.1109/SP46215.2023.10179304](https://doi.org/10.1109/SP46215.2023.10179304)
> “A taxonomy of 107 unique attack vectors related to OSS supply chains.”

**Osprey consequence:** the threat model covers source, maintainer, forge, build, registry, mirror, resolver, and consumer—not just malicious source files.

### <a id="r25"></a>R25 — Small World with High Risks

Zimmermann et al., USENIX Security 2019. **Peer-reviewed.** [USENIX paper](https://www.usenix.org/system/files/sec19-zimmermann.pdf)
> “A very small number of maintainer accounts could be used to inject malicious code into the majority of all packages.”

**Osprey consequence:** scope ownership, transfers, publisher changes, and high-impact releases receive heightened controls without popularity-based search privilege.

### <a id="r26"></a>R26 — Towards Measuring Supply Chain Attacks on Package Managers for Interpreted Languages

Duan et al., NDSS 2021. **Peer-reviewed.** [DOI 10.14722/ndss.2021.23055](https://doi.org/10.14722/ndss.2021.23055)
> “Package manager maintainers confirmed 278 (82%) from the 339 reported packages.”

**Osprey consequence:** submission analysis combines metadata, static analysis, sandboxed dynamic behavior, and human escalation.

### <a id="r27"></a>R27 — Backstabber's Knife Collection

Ohm et al., DIMVA 2020. **Peer-reviewed.** [DOI 10.1007/978-3-030-52683-2_2](https://doi.org/10.1007/978-3-030-52683-2_2)
> “A dataset of 174 malicious software packages ... distributed via npm, PyPI, and RubyGems.”

**Osprey consequence:** evaluation uses real package attack families and tests source injection, build injection, name deception, takeover, and execution timing.

### <a id="r28"></a>R28 — Investigating Package Related Security Threats in Software Registries

Gu et al., IEEE Symposium on Security and Privacy 2023. **Peer-reviewed.** [DOI 10.1109/SP46215.2023.10179332](https://doi.org/10.1109/SP46215.2023.10179332)
> “We identify twelve potential attack vectors, with six of them disclosed for the first time.”

**Osprey consequence:** names never return to the pool, releases are never replaced, mirrors cannot override origin, and case/confusable rules are canonical.

### <a id="r29"></a>R29 — DONAPI: Malicious NPM Packages Detector Using Behavior Sequence Knowledge Mapping

Huang et al., USENIX Security 2024. **Peer-reviewed.** [USENIX publication](https://www.usenix.org/conference/usenixsecurity24/presentation/huang-cheng)
> “An automatic malicious npm packages detector that combines static and dynamic analysis.”

**Osprey consequence:** deterministic analysis screens first; a network-denied sandbox then confirms behavior that static inspection cannot resolve.

### <a id="r30"></a>R30 — Archiving and Referencing Source Code with Software Heritage

Di Cosmo, ICMS 2020. **Peer-reviewed.** [DOI 10.1007/978-3-030-52200-1_36](https://doi.org/10.1007/978-3-030-52200-1_36)
> “Intrinsic persistent identifiers ... reference it at various granularities.”

**Osprey consequence:** the registry preserves canonical source snapshots and provenance independently of continued forge availability.

### <a id="r66"></a>R66 — LastPyMile: Identifying the Discrepancy between Sources and Packages

Vu et al., ESEC/FSE 2021. **Peer-reviewed.** [DOI 10.1145/3468264.3468592](https://doi.org/10.1145/3468264.3468592)
> “Such convenient practice assumes that there are no discrepancies between source code and packages.”

**Osprey consequence:** publishers identify source; the registry independently collects, archives, and assembles it instead of accepting a publisher-built package.

### <a id="r31"></a>R31 — Software Distribution Transparency and Auditability

Hof and Carle, 2017. **Preprint — not peer-reviewed.** [arXiv:1711.07278](https://arxiv.org/abs/1711.07278)
> “We introduce tree root cross logging, where the log's Merkle tree root is submitted into a separately operated log server.”

**Osprey consequence:** publication events form an append-only Merkle log whose signed roots are independently witnessed and cross-logged against equivocation.

## 3. Maintenance evidence, testing, static analysis, and AI

### <a id="r32"></a>R32 — Do Software Security Practices Yield Fewer Vulnerabilities?

Zahan et al., ICSE-SEIP 2023. **Peer-reviewed.** [DOI 10.1109/ICSE-SEIP58684.2023.00032](https://doi.org/10.1109/ICSE-SEIP58684.2023.00032)
> “Vulnerability count and security score data [should] be refined.”

**Osprey consequence:** raw historical vulnerability count is disclosed but does not automatically lower maintenance score; response and unresolved reachable risk matter.

### <a id="r33"></a>R33 — Identifying Unmaintained Projects in GitHub

Coelho et al., ESEM 2018. **Peer-reviewed.** [DOI 10.1145/3239235.3240501](https://doi.org/10.1145/3239235.3240501)
> “The proposed machine learning approach has a precision of 80% ... and a recall of 96%.”

**Osprey consequence:** continuity risk is probabilistic evidence with confidence and timestamp, never a categorical accusation of abandonment.

### <a id="r34"></a>R34 — On the Abandonment and Survival of Open Source Projects

Avelino et al., ESEM 2019. **Peer-reviewed.** [DOI 10.1109/ESEM.2019.8870181](https://doi.org/10.1109/ESEM.2019.8870181)
> “The motivation and difficulties faced when assuming an abandoned project.”

**Osprey consequence:** continuity evidence rewards documented succession, multiple authorized maintainers, and recoverable release processes—not commit volume.

### <a id="r35"></a>R35 — Lags in the Release, Adoption, and Propagation of npm Vulnerability Fixes

Chinthanet et al., *Empirical Software Engineering* 26, 2021. **Peer-reviewed.** [DOI 10.1007/s10664-021-09951-x](https://doi.org/10.1007/s10664-021-09951-x)
> “Developers are slow to respond ... sometimes taking four to eleven months to act.”

**Osprey consequence:** maintenance evidence includes median and tail response time, fix-release time, propagation, recurrence, and unresolved severity.

### <a id="r36"></a>R36 — Coverage Is Not Strongly Correlated with Test Suite Effectiveness

Inozemtseva and Holmes, ICSE 2014. **Peer-reviewed.** [DOI 10.1145/2568225.2568271](https://doi.org/10.1145/2568225.2568271)
> “Coverage ... should not be used as a quality target.”

**Osprey consequence:** coverage is a low-weight under-testing signal and is always shown with tool, scope, exclusions, and date.

### <a id="r37"></a>R37 — Are Mutants a Valid Substitute for Real Faults in Software Testing?

Just et al., FSE 2014. **Peer-reviewed.** [DOI 10.1145/2635868.2635929](https://doi.org/10.1145/2635868.2635929)
> “A statistically significant correlation between mutant detection and real fault detection, independently of code coverage.”

**Osprey consequence:** mutation score carries more test-effectiveness weight than coverage, with operators, equivalent-mutant handling, timeouts, and denominator disclosed.

### <a id="r38"></a>R38 — Effectiveness of Static C Code Analyzers for Vulnerability Detection

Lipp, Banescu, and Pretschner, ISSTA 2022. **Peer-reviewed.** [DOI 10.1145/3533767.3534380](https://doi.org/10.1145/3533767.3534380)
> “State-of-the-art tools miss in-between 47% and 80% of the vulnerabilities.”

**Osprey consequence:** static-analysis results are normalized by tool and ruleset and remain one evidence family among several.

### <a id="r39"></a>R39 — Lessons from Building Static Analysis Tools at Google

Sadowski et al., *Communications of the ACM* 61, 2018. **Peer-reviewed.** [DOI 10.1145/3188720](https://doi.org/10.1145/3188720)
> “For a static analysis project to succeed, developers must feel they benefit from and enjoy using it.”

**Osprey consequence:** findings are precise, explainable, suppressible with audited justification, and integrated into the publish workflow instead of producing opaque score deductions.

### <a id="r40"></a>R40 — Vulnerability Detection with Code Language Models: How Far Are We?

Ding et al., ICSE 2025. **Peer-reviewed.** [DOI 10.1109/ICSE55347.2025.00038](https://doi.org/10.1109/ICSE55347.2025.00038)
> “A state-of-the-art 7B model scored 68.26% F1 on BigVul but only 3.09% F1 on PrimeVul.”

**Osprey consequence:** AI promotion requires deduplicated temporal holdouts, realistic class balance, adversarial tests, calibration, and fixed operating-point metrics.

### <a id="r41"></a>R41 — Leveraging Large Language Models to Detect npm Malicious Packages

Zahan et al., ICSE 2025. **Peer-reviewed.** [DOI 10.1109/ICSE55347.2025.00146](https://doi.org/10.1109/ICSE55347.2025.00146)
> “Existing malicious code detection techniques demand the integration of multiple tools.”

**Osprey consequence:** the model consumes deterministic findings and code diffs as evidence; it does not replace provenance, static, or dynamic controls.

### <a id="r42"></a>R42 — Evaluating LLM-Based Detection of Malicious Package Updates in npm

Wyss et al., RAID 2025. **Peer-reviewed.** [DOI 10.1109/RAID67961.2025.00047](https://doi.org/10.1109/RAID67961.2025.00047)
> “Mild code obfuscations ... uniquely challenge tested LLMs.”

**Osprey consequence:** every candidate model faces supported obfuscation and prompt-injection suites; model output can quarantine for review but cannot certify safety.

### <a id="r43"></a>R43 — MalwareBench: Malware Samples Are Not Enough

Zahan et al., MSR 2024. **Peer-reviewed.** [DOI 10.1145/3643991.3644883](https://doi.org/10.1145/3643991.3644883)
> “A labeled dataset of 20,534 packages (of which 6,475 are malicious).”

**Osprey consequence:** evaluation includes large benign and malicious package populations and reports false positives per million, not accuracy alone.

### <a id="r44"></a>R44 — Dos and Don'ts of Machine Learning in Computer Security

Arp et al., USENIX Security 2022. **Peer-reviewed.** [USENIX publication](https://www.usenix.org/conference/usenixsecurity22/presentation/arp)
> “Subtle pitfalls ... undermine its performance and render learning-based systems potentially unsuitable for security tasks.”

**Osprey consequence:** the AI gate documents sampling, labels, leakage, base rates, threat model, deployment drift, cost, latency, abstention, and failure handling.

### <a id="r45"></a>R45 — On Calibration of Modern Neural Networks

Guo et al., ICML 2017. **Peer-reviewed.** [PMLR paper](https://proceedings.mlr.press/v70/guo17a.html)
> “Modern neural networks are poorly calibrated.”

**Osprey consequence:** security probabilities are calibrated on a held-out temporal set and exposed with uncertainty; thresholds are versioned policy, not universal truth.

## 4. Fair discovery and resistance to popularity capture

### <a id="r46"></a>R46 — Fairness of Exposure in Rankings

Singh and Joachims, KDD 2018. **Peer-reviewed.** [DOI 10.1145/3219819.3220088](https://doi.org/10.1145/3219819.3220088)
> “Ranking systems have a responsibility not only to their users but also to the items being ranked.”

**Osprey consequence:** default search jointly optimizes query utility and auditable supplier exposure under explicit constraints.

### <a id="r47"></a>R47 — Equity of Attention: Amortizing Individual Fairness in Rankings

Biega, Gummadi, and Weikum, SIGIR 2018. **Peer-reviewed.** [DOI 10.1145/3209978.3210063](https://doi.org/10.1145/3209978.3210063)
> “Attention accumulated across a series of rankings is proportional to accumulated relevance.”

**Osprey consequence:** fairness is measured over a rolling query-intent cohort ledger, rather than forcing every single result page into parity.

### <a id="r48"></a>R48 — Fairness of Exposure in Stochastic Bandits

Wang et al., ICML 2021. **Peer-reviewed.** [PMLR paper](https://proceedings.mlr.press/v139/wang21b.html)
> “The conventional bandit formulation can lead to an undesirable and unfair winner-takes-all allocation of exposure.”

**Osprey consequence:** a bounded exploration share goes to relevant, security-eligible, underexposed packages; exploitation alone cannot monopolize discovery.

### <a id="r49"></a>R49 — Blockbuster Culture's Next Rise or Fall

Fleder and Hosanagar, *Management Science* 55, 2009. **Peer-reviewed.** [DOI 10.1287/mnsc.1080.0974](https://doi.org/10.1287/mnsc.1080.0974)
> “Some well known recommenders can lead to a reduction in sales diversity.”

**Osprey consequence:** popularity is an optional user-selected sort, never an input to the default relevance ranking.

### <a id="r50"></a>R50 — How Algorithmic Confounding Increases Homogeneity and Decreases Utility

Chaney, Stewart, and Engelhardt, RecSys 2018. **Peer-reviewed.** [DOI 10.1145/3240323.3240370](https://doi.org/10.1145/3240323.3240370)
> “Using data confounded in this way homogenizes user behavior without increasing utility.”

**Osprey consequence:** click models train only from propensity-logged randomized exposure; organic clicks cannot silently amplify the current ranking.

### <a id="r51"></a>R51 — On the Diversity of Software Package Popularity Metrics

Zerouali et al., SANER 2019. **Peer-reviewed.** [DOI 10.1109/SANER.2019.8667997](https://doi.org/10.1109/SANER.2019.8667997)
> “Popularity can be measured with different unrelated metrics.”

**Osprey consequence:** downloads, stars, dependents, and recency are shown separately and never collapsed into an authoritative quality signal.

### <a id="r52"></a>R52 — Six Million (Suspected) Fake Stars on GitHub

He et al., ICSE 2026. **Peer-reviewed.** [DOI 10.1145/3744916.3764531](https://doi.org/10.1145/3744916.3764531)
> “The majority of fake stars are used to promote short-lived phishing malware repositories.”

**Osprey consequence:** external stars do not affect eligibility, trust, maintenance, or default rank and are marked as unaudited popularity metadata.

### <a id="r53"></a>R53 — Feedback Loop and Bias Amplification in Recommender Systems

Mansoury et al., CIKM 2020. **Peer-reviewed.** [DOI 10.1145/3340531.3412152](https://doi.org/10.1145/3340531.3412152)
> “A few popular items are recommended frequently while the majority of other items are ignored.”

**Osprey consequence:** discovery dashboards publish coverage, exposure Gini, publisher concentration, exposure-to-merit ratios, new-package exposure, and utility cost.

## 5. Standards and other official sources

### <a id="r54"></a>R54 — The Update Framework Specification 1.0.35

The Update Framework project. **Official specification.** [TUF 1.0.35](https://theupdateframework.github.io/specification/v1.0.35/index.html)
> “There are four fundamental top-level roles ... Root ... Targets ... Snapshot ... Timestamp.”

**Osprey consequence:** clients implement those separated roles, threshold verification, delegation, expiration, rollback protection, and consistent snapshots.

### <a id="r55"></a>R55 — SLSA Specification 1.2

OpenSSF SLSA project. **Official specification, approved.** [SLSA v1.2](https://slsa.dev/spec/v1.2/)
> “Build L3 ... Hardened build platform.”

**Osprey consequence:** registry builders use Build L3 hardened-platform and protected-provenance controls; hermetic inputs and reproducibility remain separate Osprey requirements.

### <a id="r56"></a>R56 — in-toto Attestation Framework 1.2.0

in-toto project. **Official specification.** [Attestation Framework v1.2.0](https://github.com/in-toto/attestation/blob/v1.2.0/spec/README.md)
> “An in-toto attestation is authenticated metadata about one or more software artifacts.”

**Osprey consequence:** evidence uses digest-bound subjects, typed predicates, authenticated envelopes, and bundles that policy engines can validate.

### <a id="r57"></a>R57 — NIST Secure Software Development Framework 1.1

NIST SP 800-218. **Official US government standard guidance.** [NIST publication](https://csrc.nist.gov/pubs/sp/800/218/final)
> “Address the root causes of vulnerabilities to prevent future recurrences.”

**Osprey consequence:** maintenance evidence includes recurrence and corrective-process evidence, not merely whether a single advisory was closed.

### <a id="r58"></a>R58 — 2025 Minimum Elements for a Software Bill of Materials

CISA. **Official US government public-comment draft.** [CISA public-comment draft](https://www.cisa.gov/sites/default/files/2025-08/2025_CISA_SBOM_Minimum_Elements.pdf)
> “An SBOM should include information for all components ... including transitive dependencies.”

**Osprey consequence:** each resolved application and published witness lock exports a complete transitive SBOM tied to immutable release identities.

### <a id="r59"></a>R59 — SPDX Specification 3.0.1

Linux Foundation SPDX project; ISO/IEC 5962 lineage. **Official open standard.** [SPDX 3.0.1 scope](https://spdx.github.io/spdx-spec/v3.0.1/scope/)
> “The System Package Data Exchange (SPDX®) specification defines an open standard for communicating bill of materials (BOM) information for different topic areas.”

**Osprey consequence:** SBOM, license, provenance, integrity, relationship, and vulnerability exports use SPDX 3.0.1 JSON-LD.

### <a id="r60"></a>R60 — Principles for Package Repository Security

OpenSSF Securing Software Repositories Working Group, 2024. **Official foundation guidance.** [Principles](https://repos.openssf.org/principles-for-package-repository-security.html)
> “Prevent specific versions of a package from being replaced.”

**Osprey consequence:** releases are immutable; removal becomes signed yank or revocation metadata, and names and versions are never recycled.

### <a id="r61"></a>R61 — Open Source Project Security Baseline

OpenSSF Security Baseline SIG, current version 2026.02.19. **Official foundation control baseline.** [OSPS Baseline](https://baseline.openssf.org/)
> “The baseline is not intended to be used as a scoring or grading mechanism.”

**Osprey consequence:** baseline controls appear as individual pass/fail/unknown evidence; the maintenance score does not misrepresent baseline conformance as safety.

### <a id="r62"></a>R62 — Go Modules Reference: Authenticating Modules

The Go project. **Official project reference documentation.** [Go Modules Reference](https://go.dev/ref/mod#authenticating)
> “It is a Transparent Log (or ‘Merkle Tree’) of `go.sum` line hashes.”

**Osprey consequence:** clients require inclusion and consistency proofs for release-log entries and reject digest changes before adding content to the cache.

### <a id="r63"></a>R63 — Open Source Vulnerability Schema 1.8.0

OpenSSF OSV project. **Official schema.** [OSV schema v1.8.0](https://github.com/ossf/osv-schema/blob/v1.8.0/README.md)
> “The OSV schema provides a human and machine-readable format to describe vulnerabilities that map precisely to open source package versions or commit hashes.”

**Osprey consequence:** advisories use OSV records; Osprey digest, epoch, ordinal, reachability, and revocation fields live under `ecosystem_specific`.

### <a id="r65"></a>R65 — An Architecture for Trustworthy and Transparent Digital Supply Chains

Birkholz et al., RFC 9943, June 2026. **Official IETF standard.** [RFC 9943](https://www.rfc-editor.org/rfc/rfc9943.html)
> “Transparency does not prevent dishonest or compromised Issuers, but it holds them accountable.”

**Osprey consequence:** attestations need append-only receipts, monitors, witnesses, and auditable policy; a transparency log is accountability evidence, not prevention.

## Reading the evidence correctly

The corpus rejects three shortcuts. No version label, signature, score, test, or
model output means “safe.” Unknown evidence is not failure or silently zero.
Fairness increases opportunity only above the same security floor. Osprey
therefore publishes evidence, methods/tools, timestamps, uncertainty, and policy beside every derived verdict or score.
