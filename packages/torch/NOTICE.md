# @osprey/torch: licensing and attribution

Original Osprey contributions are licensed under the MIT license in [LICENSE](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia. The pre-existing Christian Findlay copyright is retained there. This grant covers our original contributions; it does not replace third-party ownership or terms.

The cumulative package license expression is `MIT AND BSD-3-Clause`. `AND` requires compliance with every applicable license; it is not a choice of licenses. Upstream names identify provenance and do not imply endorsement.

These files are Osprey adaptations of the components below. Changes include translation into Osprey's Python flavor, immutable data structures, explicit handlers where used, and the documented API subset. Generated character tables are derived data. Preserve this change summary, the license texts and the applicable copyright notices in source distributions and in the documentation/materials supplied with compiled distributions.

## torch

Upstream: [pytorch/pytorch](https://github.com/pytorch/pytorch). Reference: upstream audit snapshot. Retained text: [LICENSES/torch.txt](LICENSES/torch.txt). [License source](https://github.com/pytorch/pytorch/blob/v2.14.1/LICENSE).

## PyTorch-NOTICE

Upstream: [pytorch/pytorch](https://github.com/pytorch/pytorch). Reference: upstream audit snapshot. Retained text: [LICENSES/PyTorch-NOTICE.txt](LICENSES/PyTorch-NOTICE.txt). [License source](https://github.com/pytorch/pytorch/blob/v2.14.1/NOTICE).

The complete upstream PyTorch NOTICE is preserved. References there to bundled native libraries describe upstream PyTorch; those binaries and third_party submodules are not shipped by this Osprey port.

## Distribution

The package publisher includes `LICENSE`, `NOTICE.md` and the full `LICENSES/` tree in the digest-pinned release. Fetch/install retain those files. Applications distributing compiled code must also ship the applicable license and notice files; compilation does not carry these documents into an executable automatically.
