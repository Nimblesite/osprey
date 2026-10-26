---
layout: page
permalink: /docs/releases/
title: Releases
description: Every published Osprey release, newest first, with links to its notes and downloads on GitHub.
tags:
- releases
- versions
---

Every published Osprey release, newest first. Each version links to its notes and downloads. The full release history is also on [GitHub Releases](https://github.com/Nimblesite/osprey/releases).

{% if releases.list.length %}
| Version | Released | |
| --- | --- | --- |
{% for r in releases.list -%}
| [{{ r.tag }}]({{ r.url }}){% if r.prerelease %} <sup>pre-release</sup>{% endif %} | {{ r.date }} | {% if loop.first %}Latest{% endif %} |
{% endfor %}
{% else %}
The release list was unavailable when this page was built.
{% endif %}

To install the latest release, follow [Installing Osprey](/docs/installation/). [Feature status](/status/) describes what each part of the language supports today.
