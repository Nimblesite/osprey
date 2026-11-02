# @osprey/chromecast: licensing and upstream acknowledgement

The Osprey implementation is provided under the normal [MIT license](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia, retaining the earlier Christian Findlay notice. This does not claim ownership of any third-party material identified below.

## Upstream API and behavior references

The package ports a stdlib-only CPython CAST v2 sender script written for this project. Its framing, parsing, discovery and playback decisions are the test oracle in `scripts/oracles/chromecast.py`. The CAST v2 wire format and the Default Media Receiver application id are Google's protocol; no Google code is bundled or linked.

## Specifically retained third-party material

None identified. The cumulative expression is `MIT`.

## Distribution

The publisher carries `LICENSE` and this acknowledgement in the digest-pinned release. Upstream project names provide acknowledgement and imply no endorsement.
