# Osprey CMS

A content management system whose only database is a git repository, written in TypeScript-flavor Osprey on the [Express port](../../packages/express/README.md). A developer builds a site with any generator (Eleventy, Next.js, Astro, plain HTML) and hands the repository over. Editors then change only what the site exposes, Markdown pages and media, in a browser. **Save is a git commit; Publish is a git push**, and the site's own pipeline deploys what was pushed. The contract is [GitCms.md](../../../../docs/specs/applications/GitCms.md).

The editor is an Osprey Native app. Pages open in a Word-style **Formatted** mode, where each block edits in place and a ribbon styles it (Body, H1–H3, bold, italic, code, list, quote, link, image), or in **Markdown** mode beside a live preview. The ribbon acts on the selection, and pressing a tool again turns it off. Enter starts a new paragraph. Unsaved words are never dropped silently: choosing another file asks first, and Delete asks for confirmation.

```sh
make -C src/toolchain/targets/web cms   # the editor on throwaway sites: :4397 (fixture), :4398 (the Eleventy sample)
```

To run it on your own site, point `CMS_REPO` at a git working tree with an upstream (publishing pushes to it):

```sh
make build
target/release/osprey src/osprey/examples/cms --compile -o target/cms/cms
CMS_REPO=/path/to/site CMS_ROOTS=content,media target/cms/cms
open http://127.0.0.1:4321/
```

| Folder | What it holds |
| --- | --- |
| [`src/`](src) | The server: path safety, git, the tree, chunked uploads and the routes |
| [`editor/`](editor) | The Osprey Native back-office editor, built for the browser host |
| [`site/`](site) | The sample Eleventy site the tests hand over |
| [`tests/`](tests) | The API suite (`make -C src/toolchain/targets/tests cms`) and the Chromium journeys (`make -C src/toolchain/targets/web/tests cms`) |
