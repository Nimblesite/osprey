# @osprey/scipy: licensing and attribution

Original Osprey contributions are licensed under the MIT license in [LICENSE](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia. The pre-existing Christian Findlay copyright is retained there. This grant covers our original contributions; it does not replace third-party ownership or terms.

The cumulative package license expression is `MIT AND BSD-3-Clause AND BSL-1.0`. `AND` requires compliance with every applicable license; it is not a choice of licenses. Upstream names identify provenance and do not imply endorsement.

These files are Osprey adaptations of the components below. Changes include translation into Osprey's Python flavor, immutable data structures, explicit handlers where used, and the documented API subset. Generated character tables are derived data. Preserve this change summary, the license texts and the applicable copyright notices in source distributions and in the documentation/materials supplied with compiled distributions.

## scipy

Upstream: [scipy/scipy](https://github.com/scipy/scipy). Reference: 1.18.1. Retained text: [LICENSES/scipy.txt](LICENSES/scipy.txt). [License source](https://files.pythonhosted.org/packages/7e/74/66de6258867beb2ef08f35f9f2ac017a52cacd5081714d239ff1a442d458/scipy-1.18.1.tar.gz).

## xsf

Upstream: [scipy/xsf](https://github.com/scipy/xsf). Reference: upstream audit snapshot. Retained text: [LICENSES/xsf.txt](LICENSES/xsf.txt). [License source](https://github.com/scipy/xsf/blob/main/LICENSE).

## Cephes

Upstream: [scipy/xsf](https://github.com/scipy/xsf). Reference: upstream audit snapshot. Retained text: [LICENSES/Cephes.txt](LICENSES/Cephes.txt). [License source](https://github.com/scipy/xsf/blob/main/LICENSES_bundled.txt).

## Boost-1.0

Upstream: [boostorg/boost](https://github.com/boostorg/boost). Reference: upstream audit snapshot. Retained text: [LICENSES/Boost-1.0.txt](LICENSES/Boost-1.0.txt). [License source](https://github.com/boostorg/boost/blob/master/LICENSE_1_0.txt).

## Retained source attribution

- (C) Copyright John Maddock 2006.
- Copyright 1984, 1987 by Stephen L. Moshier
- Copyright 1984, 1987, 1989 by Stephen L. Moshier
- Copyright 1984, 1987, 1989, 1992 by Stephen L. Moshier
- Copyright 1984, 1987, 1992, 2000 by Stephen L. Moshier
- Copyright 1985, 1987 by Stephen L. Moshier

The Cephes numerical coefficients and kernels retain Stephen L. Moshier's BSD terms as documented by SciPy/xsf. The digamma rational approximation retains John Maddock's Boost Software License 1.0; it is not MIT-only.

## Distribution

The package publisher includes `LICENSE`, `NOTICE.md` and the full `LICENSES/` tree in the digest-pinned release. Fetch/install retain those files. Applications distributing compiled code must also ship the applicable license and notice files; compilation does not carry these documents into an executable automatically.
