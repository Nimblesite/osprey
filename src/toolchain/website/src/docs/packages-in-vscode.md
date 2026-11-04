---
layout: page
video: package-manager
eleventyComputed:
  title: "{{ (video | video).name }}"
description: Browse Osprey packages, inspect dependency and security information, and add or remove dependencies in the VS Code extension.
permalink: /docs/packages-in-vscode/
tags: [packages, vscode, registry]
---

Open **Osprey: Manage Packages** from the Command Palette. The package workspace lets you browse the configured registry, inspect a package and change your project's dependencies without leaving VS Code.

{% videoPlayer video %}

## Open your project's packages

Open a trusted project containing `osprey.toml`. Save or revert edits to the manifest and lock before changing dependencies. The extension must include its matching package client and workspace application; package commands work with the language server disabled.

Run **Osprey: Manage Packages** with **Command+Shift+P** on macOS or **Ctrl+Shift+P** on Windows/Linux. In a multi-folder window, choose the project to change. Check the project shown in the workspace before adding or removing anything.

The registry connection is configured through `osprey.packages.registry`. This walkthrough starts with that connection already set up. Registry administration and source-build commands belong in the [package-manager guide](/docs/package-manager/).

## Browse the catalog

Choose **Browse registry** to view available packages. Scroll through the catalog or filter by name. Package identities include a scope, such as `@osprey/numpy`; the same leaf name can occur under different scopes.

Select a package to inspect its release and security findings. A release identifier such as `e1.r1` means epoch 1, release ordinal 1. The lock records exact releases and source digests.

![The Osprey package workspace in VS Code showing the registry list, selected einops release and its unverified audit warning.](/assets/images/packages/workspace-details.jpg)

## Add a package

1. Select the package in **Browse registry**.
2. Review its release and security findings.
3. Choose **Add to project**.
4. Read the confirmation, check the package and project, then confirm the addition or cancel.
5. Open **Installed** to inspect the resulting dependency graph.

![The actual add confirmation names the einops package, registry and project, and warns that no audit has been verified.](/assets/images/packages/workspace-add-confirm.jpg)

An **unverified** audit status means there is no authenticated audit evidence. An empty findings list does not establish that a package has been audited. Resolve reported blocking findings before proceeding; the interface can refuse an addition when its security requirements are not met.

The workspace updates `osprey.toml`, resolves `osprey.lock`, then fetches and verifies the locked sources before refreshing **Installed**. A failed download or verification remains an error. Commit the manifest and lock together. The CLI exposes these steps separately; see [fetching and locked builds](/docs/package-manager/#resolve-fetch-then-build-offline).

![Installed shows einops as a direct dependency, its exact source digest and zero missing packages, while the audit warning remains visible.](/assets/images/packages/workspace-installed.jpg)

Digest verification detects changed source; it does not establish publisher identity or benign code. Raw native `extern` declarations remain a confinement gap. Read the [supply-chain boundaries](/docs/package-manager/#supply-chain-controls-and-remaining-work) before relying on untrusted dependencies.

## Remove a package

Select a direct dependency in **Installed**, choose **Remove dependency**, then review and confirm the removal. The client removes the declaration and relocks the remaining graph. Update any source imports that used the removed package.

![The Remove dependency confirmation names the package and project before changing the manifest and lock.](/assets/images/packages/workspace-remove-confirm.jpg)

A transitive dependency belongs to another package's requirements. Inspect **Required by** before changing the graph; it is not an independent direct dependency to remove.

## When package data is unavailable

A loading or failed request does not establish that the project has zero packages or no security findings. The workspace must report the failure rather than present unknown data as a clean result.

If an Add operation is interrupted during relocking, its dependency declaration can remain in `osprey.toml` without a matching lockfile. Inspect both files and restore the intended dependency declarations before retrying; do not assume the interrupted change rolled back.

Check the registry connection and extension installation, then choose **Refresh**. Save or revert unsaved manifest and lock editors before retrying a mutation. The current release and platform limits, including hosted publishing and audit-service availability, are documented in the [package-manager guide](/docs/package-manager/).

## Video transcript

{% videoTranscript video %}
