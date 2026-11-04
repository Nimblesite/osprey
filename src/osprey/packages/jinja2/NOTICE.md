# @osprey/jinja2: licensing and upstream acknowledgement

The Osprey implementation is provided under the normal [MIT license](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia, retaining the earlier Christian Findlay notice. This does not claim ownership of any third-party material identified below.

## Upstream API and behavior references

These repositories and releases are acknowledged as API, behavior and test-oracle references. Their licenses are not assigned to this implementation merely because the API is reimplemented. The port does not bundle or link the upstream Python package.

- [jinja](https://github.com/pallets/jinja): 3.1.6.

## Specifically retained third-party material

`tests/fixtures/qwen3.8-27b.chat_template.jinja` comes from [Qwen3.8-27B](https://huggingface.co/Qwen/Qwen3.8-27B/tree/1d4bf0f2ff6012fd82039f2fa52739d0dd7c60c0) by the Qwen team, Alibaba Cloud, under the Apache-2.0 license. See [`LICENSES/Qwen3.8-27B-Apache-2.0.txt`](LICENSES/Qwen3.8-27B-Apache-2.0.txt). The cumulative expression is `MIT AND Apache-2.0`.

## License scope

No additional upstream software license is imposed based solely on the API name, mathematical behavior, output of an oracle, short compatibility diagnostics or the label “port”. No substantive shared implementation block was identified in the recorded comparison. A matching API does not establish either copying or independent authorship; review any future incorporation of upstream source or data separately.

## Distribution

The publisher carries `LICENSE`, this acknowledgement and any required `LICENSES/` files in the digest-pinned release. Source or compiled redistributions containing the specifically retained material must carry its applicable notices. Upstream project names provide acknowledgement and imply no endorsement.
