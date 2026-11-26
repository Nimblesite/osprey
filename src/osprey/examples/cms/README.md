# Osprey CMS

A content management system whose only database is a git repository, written in TypeScript-flavor Osprey on the [Express port](../../packages/express/README.md). A developer builds a site with any generator (Eleventy, Next.js, Astro, plain HTML) and hands the repository over. The editor then changes only what the site exposes, Markdown pages and media, in a browser. Every change is a git commit, and the site's own pipeline deploys it. The contract is [GitCms.md](../../../../docs/specs/applications/GitCms.md).

```sh
make build
target/release/osprey src/osprey/examples/cms --compile -o target/cms/cms
CMS_REPO=/path/to/site CMS_ROOTS=content,media target/cms/cms
open http://127.0.0.1:4321/
```

| Folder | What it holds |
| --- | --- |
| [`src/`](src) | The server: path safety, git, the tree, chunked uploads and the routes |
| [`editor/`](editor) | The back-office page the editor uses |
| [`site/`](site) | The sample Eleventy site the tests hand over |
| [`tests/`](tests) | The end-to-end test, run by `make -C src/osprey/examples/cms e2e` |
