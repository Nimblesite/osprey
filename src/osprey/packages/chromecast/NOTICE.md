# @osprey/chromecast: licensing and upstream acknowledgement

The Osprey implementation is provided under the normal [MIT license](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia. This does not claim ownership of any third-party material identified below.

## Upstream API and behavior references

There is no upstream repository. The package ports `cast.py`, a stdlib-only CPython CAST v2 sender that Claude (Anthropic's model) wrote for the maintainer on 31 October 2026, working from its own knowledge of the protocol. No third-party library was installed, downloaded or copied to write it. The script was never published on its own; its pure functions survive in the test oracle `src/osprey/packages/chromecast/oracle/chromecast.cpython`, which is part of this package and under its MIT license.

The CAST v2 wire format and the Default Media Receiver application id `CC1AD845` belong to Google's Cast protocol. No Google code is bundled or linked, and Chromecast and Google TV are trademarks of Google LLC, named only to say what the package talks to. Open-source senders such as [pychromecast](https://github.com/home-assistant-libs/pychromecast) (MIT) and [catt](https://github.com/skorokithakis/catt) (BSD-2-Clause) document the same protocol publicly; no code from them is included.

## Specifically retained third-party material

None identified. The cumulative expression is `MIT`.

## Distribution

The publisher carries `LICENSE` and this acknowledgement in the digest-pinned release. Upstream project names provide acknowledgement and imply no endorsement.
