# Package Evidence and Fair Discovery

**Status:** normative target. This specification makes the maintenance and
discovery rules in [Registry Trust and Discovery](0030-PackageRegistryTrustAndDiscovery.md)
reproducible. A conforming implementation MUST produce the same integer scores,
candidate set, and ranking from the same signed inputs. The empirical basis is
the [maintenance evidence](../package-manager-research.md#3-maintenance-evidence-testing-static-analysis-and-ai)
and [fair-discovery](../package-manager-research.md#4-fair-discovery-and-resistance-to-popularity-capture)
corpus,
especially the findings that coverage is not a quality target, mutation testing
better predicts fault detection, static tools have material blind spots, and
ranking exposure must be amortized rather than awarded by popularity.

The key words `MUST`, `MUST NOT`, `SHOULD`, and `MAY` are interpreted as BCP 14
(RFC 2119 and RFC 8174) when capitalized. There are no implementation-selected
weights, thresholds, priors, tie-breakers, or missing-data substitutions.

## Canonical arithmetic `[PACKAGE-EVIDENCE-ARITHMETIC]`

All scores use unsigned basis points: `0` is 0.00 and `10_000` is 100.00. The
display has whole part `floor(score / 100)` and two-digit fractional part
`score % 100`. Calculations use
arbitrary-precision signed integers. Floating-point arithmetic is forbidden.
For non-negative `n` and positive `d`:

```text
roundDiv(n, d)       = floor((n + floor(d / 2)) / d)
signedRoundDiv(n, d) = sign(n) * roundDiv(abs(n), d)
clamp(x, lo, hi)     = min(hi, max(lo, x))
higher(x, bad, good) = 0 when x <= bad; 10_000 when x >= good;
                       roundDiv(10_000 * (x - bad), good - bad) otherwise
lower(x, good, bad)  = 10_000 - higher(x, good, bad)
ratio(n, d)          = min(10_000, roundDiv(10_000 * n, d))
weighted(values)     = roundDiv(sum(weight[i] * value[i]), sum(weight[i]))
```

`higher` requires `good > bad`; `lower` requires `bad > good`. Units in a
formula are exact integer milliseconds, counts, bytes, or basis points. A day is
86,400,000 milliseconds. Durations are the difference between signed UTC event
times. Percentile `p` is the item at zero-based index
`ceil(p * count / 100) - 1` after ascending duration then evidence-digest order;
this specification uses only `p = 50` and `p = 90`.

Every evaluation is bound to a signed catalog `asOf`, policy digest, evidence
snapshot digest, tool/ruleset digests, and candidate-release digest. Set-like
inputs are duplicate-free and sorted by canonical UTF-8 bytes. Event lists sort
by event time, subject digest, then evidence digest. A policy bundle pins the
integer formulas, Unicode version, tokenizer, embedding model, quantized
inference runtime, intent taxonomy, vulnerability feed, static rules, mutation
operators, and conformance vectors. Changing any item creates a new policy
digest; old results remain replayable.

Malformed, expired, unauthenticated, wrong-subject, or wrong-policy evidence is
`Unknown`. It is never coerced to success or failure. A registry-run check that
conclusively finds an absent practice is an observed zero. Tool or registry
failure is `Unknown`; a package-caused timeout, resource exhaustion, invalid
test, or suppressed finding without an accepted signed justification is an
observed failure.

## Evidence confidence `[PACKAGE-EVIDENCE-CONFIDENCE]`

Each raw metric is either `Observed(score, confidence)` or `Unknown`. Confidence
also uses basis points and is computed by evidence class:

```text
Snapshot confidence = ratio(coveredSubjects, eligibleSubjects)
History confidence  = roundDiv(ratio(sampleCount, targetSamples)
                               * ratio(observedDays, 730), 10_000)
Policy confidence   = 10_000 when the registry proves pass or fail
```

Snapshot evidence with no eligible subjects has confidence `10_000` only when
the check exhaustively proves the set empty. Snapshot evidence is unknown after
its stated maximum age or when it does not cover the complete current release.
History is the half-open interval `(asOf - 730 days, asOf]`; `observedDays` starts
at the later of first publication and the window boundary and is
`floor(observedMilliseconds / 86_400_000)`. History with no qualifying event is
unknown, except open items explicitly included as right-censored samples below.
Policy checks use current signed state and do not infer a practice from text.
First publication only caps observation duration and therefore confidence; raw
package age and release-date recency never contribute a metric score.

For a component, only observed metric scores enter its weighted mean; their
declared weights are not renormalized for confidence. A component with no
observed metrics is the neutral prior `7_000`. Its confidence is the sum of
`metricWeight * metricConfidence`, divided by `10_000`; unknown metrics have
zero confidence. The five component scores and confidences are then combined
with their declared component weights:

```text
MaintenanceScore = weighted([
  (VulnerabilityResponse,     3_000),
  (Continuity,                2_500),
  (TestEffectiveness,         2_000),
  (ReviewAndStaticHygiene,    1_500),
  (CommunityResponsiveness,   1_000)])

EvidenceConfidence = weighted(component confidences with the same weights)
AdjustedMaintenance = roundDiv(
  EvidenceConfidence * MaintenanceScore
    + (10_000 - EvidenceConfidence) * 7_000,
  10_000)
```

The component vector, confidence vector, raw numerators and denominators,
unknown reasons, excluded-event digests, and formulas MUST be published. Raw
vulnerability count, downloads, dependents, stars, publication age or release
recency, commits,
contributors, issue volume, publisher revenue, and maintainer fame have weight
zero directly. Signed assessment/evidence freshness may affect SecurityEvidence.

## Vulnerability response `[PACKAGE-EVIDENCE-VULNERABILITY]`

Only an authenticated advisory mapped to reachable code in the scored release
is included. `Confirmed`, `Possible`, and `ProvenUnreachable` reachability have
factors `10_000`, `5_000`, and `0`. Severity weights and response deadlines are:
Critical `(10_000, 1 day)`, High `(6_000, 7 days)`, Medium `(3_000, 30 days)`,
and Low `(1_000, 90 days)`.

Vulnerability age begins at the earliest authenticated private-report receipt,
registry-report receipt, or upstream confirmation; delayed public disclosure
cannot reset it, and embargoed evidence is logged as a timestamped commitment.
For each unresolved advisory, `overdue = max(0, age - deadline)` and
`penalty = roundDiv(severityWeight * reachabilityFactor *
min(overdue, deadline), 10_000 * deadline)`. `OpenRisk` is
`10_000 - min(10_000, sum(penalty))`. This is known-advisory debt, not a claim
that the release has no unknown vulnerability.

| Metric | Weight | Exact score | Class / sufficiency |
| --- | ---: | --- | --- |
| OpenRisk | 3,500 | Formula above | Snapshot; feed no older than 24 hours |
| AcknowledgementMedian | 1,200 | `lower(p50 milliseconds, 1 day, 14 days)` | History; 10 advisories |
| FixP90 | 1,800 | `lower(p90 milliseconds, 7 days, 90 days)` | History; 10 advisories |
| FixReleaseP90 | 1,000 | `lower(p90 milliseconds, 1 day, 14 days)` | History; 10 fixes |
| SupportedEpochPropagationP90 | 1,200 | `lower(p90 milliseconds, 7 days, 60 days)` | History; 10 fixes |
| RootCauseRecurrence | 1,300 | `lower(ratio(recurrences, fixed advisories), 0, 3_000)` | History; 8 fixed advisories |

Acknowledgement requires a substantive triage statement, not an automated
reply. Fix time ends at the reviewed fixing commit. Fix-release time ends at
the first non-revoked release containing it. Propagation ends only when every
supported affected epoch has a fixed release or a signed immediate-EOL event;
the latter counts as the 60-day bad bound. Recurrence requires an independently
confirmed repeat of the same root cause within 365 days. An advisory awaiting
acknowledgement, fix, release, or propagation contributes its duration through
`asOf` to the corresponding percentile. Lifetime disclosure count is displayed
separately and cannot change the score.

## Continuity `[PACKAGE-EVIDENCE-CONTINUITY]`

| Metric | Weight | Exact score | Class / sufficiency |
| --- | ---: | --- | --- |
| IndependentReleaseAuthority | 2,500 | one authority `4_000`; two `8_000`; three or more `10_000` | Policy |
| RecoveryDrill | 2,500 | `lower(age of latest successful drill, 365 days, 730 days)`; none is zero | Policy |
| SupportedEpochPolicy | 1,500 | ratio of supported epochs with signed owner, support end, and migration route | Policy |
| ReleaseReliability | 2,000 | ratio of intents published within 7 days to accepted intents | History; 8 workflows |
| ResponseContinuity | 1,500 | ratio of qualifying response quarters to observed response quarters | History; 8 quarters |

Authorities are independent only when they are distinct verified people with
separate passkeys, recovery factors, and approval paths. A recovery drill must
transfer a disposable scope through the real threshold workflow and restore it
without an incumbent credential. A release workflow is an accepted signed
intent; it succeeds only if it publishes within seven days without a
publisher-caused retry. Registry-caused outages are unknown. A response quarter
is observed only when it contains an actionable report, and qualifies when its
p90 response is at most 14 days. Empty quarters are unknown. Raw maintainer and
contributor counts do not enter any formula.

## Test effectiveness `[PACKAGE-EVIDENCE-TESTING]`

All measurements are registry-recomputed against the exact release, toolchain,
test plan, and policy. They expire when any of those digests changes or after 90
days, whichever is earlier.

| Metric | Weight | Exact score | Snapshot coverage |
| --- | ---: | --- | --- |
| Mutation | 4,000 | ratio of killed mutants to executed non-equivalent mutants | all eligible mutation instances or a digest-selected 500-instance sample |
| BranchCoverage | 1,500 | ratio of covered executable branches to all executable branches | complete instrumented package |
| PropertyAndFuzz | 2,000 | zero on any reproducible failure; otherwise `ratio(completed CPU ms, 3_600_000)` | fixed one-hour campaign |
| AffectedClients | 2,500 | ratio of passing clients to selected clients | at most 50 clients |

Every candidate mutation instance has canonical ID
`u32be(pathLength)||path||u64be(startByte)||u64be(endByte)||u32be(operatorCode)`.
Accepted, signed rule-specific equivalent exclusions are removed first; the
remaining instances are eligible. When more than 500 remain, sort by
`SHA256("osprey.mutation-sample.v1\0"||u32be(len(releaseDigest))||releaseDigest||id)`,
then ID, and execute the first 500; the policy fixes operator codes.

Surviving mutants and timeouts are failures. If no tests are discovered,
Mutation, BranchCoverage, and PropertyAndFuzz are observed zeros with full
confidence. A tested package with zero eligible mutation instances after
exclusions has unknown Mutation; a
branch-free package with at least one passing test has full branch coverage.
The affected-client population contains the newest eligible release of every
other package key in the same catalog snapshot whose witness lock reaches the
candidate key at that epoch. Substitution keeps every other pin; success requires
resolution, compilation/type/effect checks, and all declared client tests (a
client with no tests still must compile). Exclusions are impossible, not opt-in.
Affected clients are the first 50 by ascending
`sha256("osprey affected client v1\0" || u32be(length(clientPackageKey)) ||
clientPackageKey || u32be(len(clientReleaseDigest)) || clientReleaseDigest ||
u32be(len(releaseDigest)) || releaseDigest)` from that population;
failure, invalidation, or timeout is non-passing. No eligible client
makes that metric unknown, which protects a new package from a fake perfect
client score.

Mutation confidence is
`roundDiv(SnapshotConfidence * ratio(min(eligibleMutations, 500), 100), 10_000)`;
AffectedClients confidence is
`roundDiv(SnapshotConfidence * ratio(eligibleClients, 20), 10_000)`. The explicit no-tests zero retains full
confidence. These sample factors prevent one mutant or one client from creating
a high-confidence perfect score.

## Review and static hygiene `[PACKAGE-EVIDENCE-STATIC]`

| Metric | Weight | Exact score | Class / sufficiency |
| --- | ---: | --- | --- |
| IndependentReviewCoverage | 3,000 | ratio of reviewed changed logical lines to all changed logical lines | History; 8 releases |
| CompilerEffectHygiene | 2,500 | `lower(milli-points per KLOC, 0, 2_000)` | Snapshot; complete source, max age 30 days |
| StaticAnalysisHygiene | 3,000 | `lower(milli-points per KLOC, 0, 10_000)` | Snapshot; complete source, max age 30 days |
| FindingRemediationP90 | 1,500 | `lower(p90 milliseconds, 3 days, 45 days)` | History; 20 findings |

An independent review is an approval by an authority other than the author and
covers the attested diff; the initial release treats every logical source line
as changed. Compiler/type/effect diagnostic points are Warning `4` and Note `1`.
Static finding points are Critical `100`, High `40`, Medium `10`, and Low `2`.
`milli-points per KLOC` is
`roundDiv(points * 1_000_000, logicalSourceLines)`; zero
logical lines is unknown. Open findings contribute their age at `asOf` to the
remediation percentile, so leaving findings open cannot improve it. Compiler
errors and unconfirmed blocking findings remain publication gates, not scores.

## Community responsiveness `[PACKAGE-EVIDENCE-COMMUNITY]`

| Metric | Weight | Exact score | Class / sufficiency |
| --- | ---: | --- | --- |
| InitialResponseMedian | 2,500 | `lower(p50 milliseconds, 2 days, 14 days)` | History; 20 reports |
| InitialResponseP90 | 2,000 | `lower(p90 milliseconds, 7 days, 45 days)` | History; 20 reports |
| ResolutionP90 | 2,500 | `lower(p90 milliseconds, 14 days, 180 days)` | History; 20 reports |
| AdvisoryMigrationCompleteness | 2,000 | ratio of qualifying changes with advisory and machine-checked migration | History; 10 changes |
| DeprecationDiscipline | 1,000 | ratio of deprecated epochs with at least 90 days' notice and a migration route | History; 5 epochs |

Reports are deduplicated, independently triaged actionable bug, security, or
documentation reports. Automated replies are not responses. Resolution means a
linked release, accepted documentation correction, or a reasoned independently
reviewed rejection; reports awaiting initial response or resolution contribute
their age at `asOf` to the applicable percentile. A qualifying change is a
breaking-epoch release, security fix, deprecation, or removal; completeness
requires a signed advisory and a migration example that compiles against the
successor. Rate-limited, coordinated, or abusive reports are excluded by a
signed moderation decision with an appeal path. Issue, comment, and reporter
counts have zero score weight.

## Eligibility and intent `[PACKAGE-DISCOVERY-INTENT]`

Default search considers one newest eligible release per package key. Eligibility
requires verified provenance and transparency, completed assessment, compatible
license/compiler/target/capabilities, and no quarantine or revocation. A yank is
eligible only for an exact locked lookup, never discovery. Failure of any gate
excludes the candidate; ranking cannot compensate for it.

Queries are Unicode NFKC-normalized, full-case-folded, tokenized by the policy
bundle, and assigned to one public finite intent-taxonomy leaf. Signed manifest
discovery metadata contains a plain valid-UTF-8 summary with no NUL/control
characters and at most 512 bytes, plus at most 12 unique keywords. A keyword
must match `[a-z0-9][a-z0-9-]{0,31}`; duplicate or out-of-order keywords reject
the manifest.

Candidate text contains only the canonical package key, keywords, summary, and
compiler-emitted fully qualified public symbol names—never arbitrary package
documentation. Symbols are unique, valid UTF-8 without controls; names over 256
bytes are not indexed. Sort them by
`sha256("osprey discovery api v1\0" || releaseDigest || u32be(length) || bytes)`,
then bytes, and append at most 128 until the next symbol would make the frozen
tokenizer exceed 8,192 tokens. The semantic input is the exact sequence
below after policy-pinned NFKC/case-folding; lexical analysis uses those same
normalized fields, and no other text enters either representation:

```text
u32be(len(key)) || key || u32be(len(summary)) || summary
|| u16be(K) || each_keyword(u32be(len(keyword)) || keyword)
|| u16be(S) || each_symbol(u32be(len(symbol)) || symbol)
```

Lengths count UTF-8 bytes. Retrieval is the union of the top 1,000 lexical and
top 1,000 semantic candidates plus an exact package-key match, deduplicated by
package key; equal retrieval scores sort by canonical package key.

For each query token, the signed catalog stores
`idf = floor(1_000_000 * ln((N + 1) / (documentFrequency + 1)))`; this value is
precomputed by the policy builder, so search performs no logarithm. A token's
best exact field weight is key `10_000`, API symbol `8_000`, keyword `7_000`,
or summary `5_000`; a prefix-only match receives half.
`Lexical` is `roundDiv(sum(idf * bestWeight), sum(idf))`, or zero when the
denominator is zero.

The pinned quantized model emits signed millionths cosine similarity.
`Semantic = higher(cosineMillionths, 150_000, 850_000)`. `ExactKey` is `10_000`
only when the entire normalized query is the canonical package key, else zero:

```text
IntentMatch = max(ExactKey, weighted([(Semantic, 6_000), (Lexical, 4_000)]))
```

Embeddings missing for a candidate exclude it until indexing completes. If
query-model inference fails globally, retrieval uses the top 2,000 lexical
candidates plus an exact key match, the response uses `IntentMatch = Lexical`
for every candidate, marks `lexical-fallback`, and contributes no learning data.

## Security evidence, compatibility, and merit `[PACKAGE-DISCOVERY-MERIT]`

`SecurityEvidence` is exact-release evidence:

```text
SecurityEvidence = weighted([
  (verified complete in-toto/SLSA provenance,       3_000),
  (bit-identical builds by two independent operators/platforms, 3_000),
  (lower(assessment age, 0, 30 days),               2_000),
  (witnessed inclusion and consistency proofs,      2_000)])
```

Boolean evidence is `10_000` or `0`; the eligibility gate already excludes a
zero provenance, reproducibility, or transparency term. This score records
evidence strength and freshness, not safety.

Without a project context, every eligible candidate has Compatibility `10_000`.
With a locked project context, the complete resolver computes the candidate
graph and the score is:

```text
penalty = 2_500 * changedCompatibilityEpochs
        + 1_500 * newSystemCapabilities
        +   100 * replacedSameEpochReleases
        +    50 * newPackageKeys
Compatibility = 10_000 - min(10_000, penalty)

Merit = weighted([(IntentMatch,             5_500),
                  (SecurityEvidence,        2_000),
                  (AdjustedMaintenance,     1_500),
                  (Compatibility,           1_000)])
```

Counts compare the current lock with the unique solved graph; an impossible
graph is ineligible. `changedCompatibilityEpochs` counts keys present in both
graphs whose epoch differs; `newSystemCapabilities` and `newPackageKeys` are set
differences; `replacedSameEpochReleases` counts shared keys with the same epoch
and a different release digest. Popularity, impressions, clicks, installs, downloads,
dependents, stars, publisher revenue, sponsorship, release-date recency, and user
identity cannot directly change Merit. Signed evidence freshness can change its
SecurityEvidence component.

## Exposure ledger `[PACKAGE-DISCOVERY-EXPOSURE]`

The public intent leaf and target class form a cohort. `PublisherId` is the
verified person or organization controlling the scope; accounts found to share
control are one publisher after a logged, appealable decision. The ledger retains the
UTC day containing `asOf` and the preceding 27 UTC days as integer
micro-exposure buckets for `(cohort, packageKey)` and stores
only expected and actual totals. Position weights for ranks 1 through 10 are
`[10_000, 7_200, 5_600, 4_500, 3_700, 3_100, 2_600, 2_200, 1_900, 1_600]`.

Package spam cannot manufacture entitlement. For publisher `p`, let
`PublisherMerit[p]` be its maximum candidate Merit and `PublisherSum[p]` its
sum. When positive, candidate `i` receives base effective merit
`floor(PublisherMerit[p] * Merit[i] / PublisherSum[p])`; distribute the remaining
units of that exact `PublisherMerit[p]` budget by descending division remainder,
then canonical key. A zero-sum publisher gives every candidate zero. Thus adding
packages cannot increase a publisher's total effective merit. The feasible result count is
`K = min(10, sum over publishers(min(2, candidateCount[p])))`, and `W` is the
sum of the first `K` position weights. Before ranking, current expected exposure
is apportioned in proportion to effective merit. If every publisher budget is
zero, only each publisher's smallest canonical key receives effective merit one.
With
`T = sum(effectiveMerit)`, candidate `i` first receives
`floor(W * effectiveMerit[i] / T)` and remainder
`(W * effectiveMerit[i]) mod T`; remaining units go by descending remainder then
canonical package key. The result totals exactly `W`.

Let `E` be retained expected exposure plus the current expectation and `A` be
retained actual exposure. The signed debt ratio and adjusted merit are:

```text
DebtRatio = clamp(signedRoundDiv((E - A) * 10_000, max(E, 1)), -10_000, 10_000)
ExposureAdjustedMerit = clamp(
  Merit + signedRoundDiv(500 * DebtRatio, 10_000), 0, 10_000)
```

Thus fairness can move Merit by at most five points. After returning a result,
its actual position weight and every candidate's current expectation are added
to the current daily bucket. A failed, cancelled, crawler, abuse-filtered, or
non-user-visible response contributes nothing.

## Top-ten allocation `[PACKAGE-DISCOVERY-ALLOCATION]`

Ranks are filled sequentially from 1 through `K`; ranks 4 and 8 are exploration
positions when present. Other ranks select the
remaining candidate with greatest ExposureAdjustedMerit, then greatest Merit,
greatest DebtRatio, and finally lexicographically smallest canonical package
key. No publisher may occupy more than two positions.

At an exploration position, the pool contains unselected candidates that keep
the publisher cap, have positive exposure debt, and satisfy
`IntentMatch * 10_000 >= bestIntentMatch * 8_000`. Selection is a without-
replacement lottery with weight `E - A` for each positive-debt candidate. A
registry VRF binds the catalog, policy, ledger snapshot, cohort, pool digest,
latest witnessed checkpoint, and a client-generated 128-bit request nonce;
rejection sampling maps its byte stream uniformly into `[0, sum(E - A))` and
canonical package-key prefix sums select the winner. The response includes the
nonce, VRF output/proof, pool digest, and exact selection propensity.

If the exploration pool is empty, that position uses the ordinary selection
rule. If the VRF is unavailable, it chooses greatest DebtRatio with the ordinary
ties, marks `deterministic-exploration-fallback`, and contributes no learning
data. If the ledger is unavailable or unverifiable, search ranks raw Merit with
the same publisher cap, disables exploration and learning, and marks
`fairness-degraded`. Missing candidates leave positions empty: no fallback may
relax eligibility, intent threshold, package uniqueness, or publisher cap.

`Most used` is a separately selected, clearly labeled sort. It does not write
the exposure ledger, train relevance, or affect any default-search score.
Pay-to-rank and blended sponsorship are forbidden in every sort.

## Privacy, audit, and deployment gates `[PACKAGE-DISCOVERY-AUDIT]`

Raw queries, query embeddings, IP addresses, account IDs, and cross-day device
identifiers MUST NOT be persisted. Infrastructure logs redact them before
write. A daily rotating opaque token permits at most one ledger contribution
per session and cohort: only the first eligible visible response atomically adds
all candidate expectations and returned-position actuals; later responses add
nothing. The token is unlinkable across days and erased within 24 hours.
Ledger and aggregate propensity buckets expire after 28 days. Optional click or
install learning uses only aggregate randomized-exploration outcomes divided by
the logged propensity; organic outcomes and deterministic fallbacks are ignored.

Every response identifies the policy, catalog, `asOf`, evidence and ledger
snapshots, mode, cohort, candidate-set digest, component scores, exclusions,
debts, tie-breaks, publisher-cap decisions, and VRF proof. Anyone can replay the
response after the VRF proof is public. Conformance requires byte-identical
golden vectors on two independent implementations.

A ranking policy deploys only when, on the signed editorial benchmark and
28-day replay simulation, NDCG@10 falls by no more than 100 absolute basis points from
the incumbent, catalog coverage@10 does not fall, exposure Gini rises by no more
than 100 basis points, no publisher exceeds 20% of top-ten exposure, and every
publisher with at least 100,000 expected micro-exposure receives at least 50% of
its entitlement. Failure of any gate keeps the incumbent policy active. These
metrics audit the algorithm; none becomes a package-ranking feature.
