---
layout: page.njk
title: Announcements and credits
description: Announcements from the Osprey blog and the authors behind the libraries and applications Osprey ports, binds or reimplements.
date: "git Last Modified"
tags: ["credits", "announcements", "attribution"]
author: "Christian Findlay"
---

Osprey's libraries and applications reimplement the APIs, formats and ideas of
remarkable open-source projects, and the language leans on several more as
development oracles. We are grateful to the authors named below: their work made
these ports possible, they retain ownership of it, and none of this is an
endorsement by them. The package audit that pins every fact is
[`src/osprey/packages/license-audit.json`](https://github.com/Nimblesite/osprey/blob/main/packages/license-audit.json).

## Announcements

The latest posts from the [blog](/blog/), newest first:

<ul>
{% for post in collections.blog %}{% if loop.index <= 8 %}
  <li><a href="{{ post.url }}">{{ post.data.title }}</a> — <time datetime="{{ post.date | htmlDateString }}">{{ post.date | readableDate }}</time></li>
{% endif %}{% endfor %}
</ul>

Earlier posts are on the [blog index](/blog/).

{% macro table(rows, licence, target) %}
<table>
<thead>
<tr><th>Project</th><th>Authors</th><th>{{ licence }}</th><th>{{ target }}</th><th>Relationship</th></tr>
</thead>
<tbody>
{% for r in rows %}
<tr{% if r.id %} data-package="{{ r.id }}"{% endif %}>
<td>{% if r.projectUrl %}<a href="{{ r.projectUrl }}">{{ r.project }}</a>{% else %}{{ r.project }}{% endif %}</td>
<td>{{ r.authors }}</td>
<td>{% if r.licenseUrl %}<a href="{{ r.licenseUrl }}">{{ r.license }}</a>{% else %}{{ r.license }}{% endif %}</td>
<td><a href="{{ r.ospreyUrl }}">{{ r.name }}</a></td>
<td>{{ r.relationship }}</td>
</tr>
{% endfor %}
</tbody>
</table>
{% endmacro %}

## Libraries

Ported and derived libraries. The licence column shows the terms of the Osprey
package, with a link where upstream licence text is retained or pointed to. Each
upstream project's own licence lives with the project linked in the first column.

{{ table(credits.ported, "Osprey licence", "Osprey package") }}

Original Osprey libraries, credited to the project itself:

{{ table(credits.original, "Osprey licence", "Osprey package") }}

## Applications and examples

Applications and examples that derive from outside work and have no package
audit entry.

{{ table(credits.apps, "Upstream licence", "Osprey app or example") }}
