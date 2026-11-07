---
mlTwins: manual
layout: page
video: agent-native
eleventyComputed:
  title: "{{ (video | video).name }}"
  description: "{{ (video | video).description }}"
permalink: /docs/osprey-native/agent/
tags: [video, native, swiftui, agents]
---

{% videoPlayer video %}

Open a tool activity group to inspect commands, file reads and edits. Review the verification result and scroll through the corrected code; usage and changed files remain in the inspector.

This film navigates an existing calculator conversation in the real macOS app. It does not make a new model request. Project names and local paths are excluded or redacted; tool results remain visible.

**The agent is not sandboxed:** it can run commands and change files your account can. Osprey is actively evolving.

From a repository checkout, after installing the build prerequisites:

```sh
make -C src/toolchain/targets/macos agent PROJECT=/path/to/your/project
```

[Explore Osprey Native →](/docs/osprey-native/) · [Agent source and setup](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/agent)

{% videoTranscript video %}
