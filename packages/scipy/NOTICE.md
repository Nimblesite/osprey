# @osprey/scipy: licensing and upstream acknowledgement

The Osprey implementation is provided under the normal [MIT license](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia, retaining the earlier Christian Findlay notice. This does not claim ownership of any third-party material identified below.

## Upstream API and behavior references

These repositories and releases are acknowledged as API, behavior and test-oracle references. Their licenses are not assigned to this implementation merely because the API is reimplemented. The port does not bundle or link the upstream Python package.

- [scipy](https://github.com/scipy/scipy): 1.18.1.
- [xsf](https://github.com/scipy/xsf): upstream audit snapshot.

## Specifically retained third-party material

The cumulative expression for the identified material is `MIT AND BSD-3-Clause AND BSL-1.0`. These additional terms apply to that material; they do not transfer ownership of the Osprey implementation or turn upstream API references into dependencies.

### xsf

Material: Translated numerical kernels and copied approximation coefficients. Locations: `src/gamma.py`, `src/beta.py`, `src/zeta.py`, `src/support.py`, `src/boxcox.py`, `src/information.py`, `src/logistic.py`.
Original source: [https://github.com/scipy/xsf](https://github.com/scipy/xsf/blob/main/LICENSE). Retained required text: [LICENSES/xsf.txt](LICENSES/xsf.txt).

### cephes

Material: Translated numerical kernels and copied approximation coefficients. Locations: `src/gamma.py`, `src/beta.py`, `src/zeta.py`, `src/support.py`, `src/boxcox.py`, `src/information.py`, `src/logistic.py`.
Original source: [https://github.com/scipy/xsf](https://github.com/scipy/xsf/blob/main/LICENSES_bundled.txt). Retained required text: [LICENSES/Cephes.txt](LICENSES/Cephes.txt).

### boost

Material: Translated numerical kernels and copied approximation coefficients. Locations: `src/gamma.py`, `src/beta.py`, `src/zeta.py`, `src/support.py`, `src/boxcox.py`, `src/information.py`, `src/logistic.py`.
Original source: [https://github.com/boostorg/boost](https://github.com/boostorg/boost/blob/master/LICENSE_1_0.txt). Retained required text: [LICENSES/Boost-1.0.txt](LICENSES/Boost-1.0.txt).

The Cephes coefficients and kernels retain Stephen L. Moshier's copyright and BSD conditions. The rational digamma approximation retains `(C) Copyright John Maddock 2006.` and the Boost Software License 1.0. Changes re-express the kernels in Osprey using immutable values, list operations and explicit result handling. The SciPy/xsf BSD notice is retained for the adapted numerical source.

## Distribution

The publisher carries `LICENSE`, this acknowledgement and any required `LICENSES/` files in the digest-pinned release. Source or compiled redistributions containing the specifically retained material must carry its applicable notices. Upstream project names provide acknowledgement and imply no endorsement.
