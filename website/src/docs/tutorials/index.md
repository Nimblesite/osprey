---
layout: page
title: "Osprey video tutorials"
description: Video tutorials for Osprey applications and tools, including VS Code package management, profiling, and native app interop.
permalink: /docs/tutorials/
tags: [tutorials, video]
---

Watch real Osprey applications and tools. Each video includes captions, chapters and a transcript; the accompanying guide explains how to use the feature.

{%- for item in collections.all | sort(false, false, "data.title") %}{% if item.data.video and item.url.startsWith("/docs/") %}{% set v = item.data.video | video %}

## {{ v.name }}

{% videoCard item.data.video, item.url %}

{{ v.description }}
{%- endif %}{% endfor %}

## More to read

The [documentation home](/docs/) lists the written guides, including [web apps](/docs/web-apps/), [iOS and Android apps](/docs/mobile-apps/) and [algebraic effects](/docs/effects/).
