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

Material: copied approximation coefficients and adapted Cephes gamma, beta, zeta, reciprocal-gamma, psi and polynomial-evaluation kernels. Locations: `src/gamma.py`, `src/beta.py`, `src/zeta.py`, `src/support.py`. Required BSD terms: [LICENSES/Cephes.txt](LICENSES/Cephes.txt).

Retained source-header attribution:

- [include/xsf/cephes/gamma.h](https://github.com/scipy/xsf/blob/473a8f81df855a1426d5148448babcbcb7696403/include/xsf/cephes/gamma.h): Copyright 1984, 1987, 1989, 1992 by Stephen L. Moshier.
- [include/xsf/cephes/beta.h](https://github.com/scipy/xsf/blob/473a8f81df855a1426d5148448babcbcb7696403/include/xsf/cephes/beta.h): Copyright 1984, 1987 by Stephen L. Moshier.
- [include/xsf/cephes/zeta.h](https://github.com/scipy/xsf/blob/473a8f81df855a1426d5148448babcbcb7696403/include/xsf/cephes/zeta.h): Copyright 1984, 1987 by Stephen L. Moshier.
- [include/xsf/cephes/zetac.h](https://github.com/scipy/xsf/blob/473a8f81df855a1426d5148448babcbcb7696403/include/xsf/cephes/zetac.h): Copyright 1984, 1987, 1989 by Stephen L. Moshier.
- [include/xsf/cephes/rgamma.h](https://github.com/scipy/xsf/blob/473a8f81df855a1426d5148448babcbcb7696403/include/xsf/cephes/rgamma.h): Copyright 1985, 1987 by Stephen L. Moshier.
- [include/xsf/cephes/polevl.h](https://github.com/scipy/xsf/blob/473a8f81df855a1426d5148448babcbcb7696403/include/xsf/cephes/polevl.h): Copyright 1984, 1987, 1988 by Stephen L. Moshier.
- [include/xsf/cephes/psi.h](https://github.com/scipy/xsf/blob/473a8f81df855a1426d5148448babcbcb7696403/include/xsf/cephes/psi.h): Copyright 1984, 1987, 1992, 2000 by Stephen L. Moshier.

### boost

Material: digamma rational coefficients (`psi_root1`, `psi_root2`, `psi_root3`, `psi_P`, `psi_Q`, `PSI_Y`) and `psi_imp_1_2` in `src/gamma.py` only. Original attribution: `(C) Copyright John Maddock 2006.` Required terms: [Boost Software License 1.0](LICENSES/Boost-1.0.txt).

Original source: [the rational approximation on [1, 2]](https://github.com/scipy/xsf/blob/473a8f81df855a1426d5148448babcbcb7696403/include/xsf/cephes/psi.h). This notice does not assign Boost terms to the beta, zeta, Box-Cox, information or logistic implementations.

Changes re-express the identified numerical code in Osprey using immutable values and list operations. All retained source ownership remains with its original authors.

## Distribution

The publisher carries `LICENSE`, this acknowledgement and any required `LICENSES/` files in the digest-pinned release. Source or compiled redistributions containing the specifically retained material must carry its applicable notices. Upstream project names provide acknowledgement and imply no endorsement.
