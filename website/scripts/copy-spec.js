const fs = require('fs');
const path = require('path');

const specSourceDir = path.resolve(__dirname, '../../docs/specs');
const specDestDir = path.resolve(__dirname, '../src/spec');

// README.md in docs/specs/ is a meta-doc for engineers, not a spec section.
// Skip it so it doesn't end up as a /spec/readme/ page on the website.
const SKIP_FILES = new Set(['README.md']);

// Ensure destination directory exists
if (!fs.existsSync(specDestDir)) {
  fs.mkdirSync(specDestDir, { recursive: true });
}

// Helper function to extract title from markdown content
function extractTitle(content, filename) {
  const lines = content.split('\n');

  // Look for H1 headers first
  for (const line of lines) {
    if (line.startsWith('# ')) {
      let title = line.substring(2).trim();
      // Remove section numbers like "1. " from the beginning
      title = title.replace(/^\d+\.\s*/, '');
      return title;
    }
  }

  // Look for H2 headers with section numbers
  for (const line of lines) {
    if (line.startsWith('## ')) {
      let title = line.substring(3).trim();
      // Remove section numbers like "1. " from the beginning
      title = title.replace(/^\d+\.\s*/, '');
      return title;
    }
  }

  // Extract from filename as fallback
  const fileTitle = filename.replace(/\.md$/, '').replace(/^\d+-/, '').replace(/([A-Z])/g, ' $1').trim();
  return fileTitle.charAt(0).toUpperCase() + fileTitle.slice(1);
}

// Helper function to clean content for individual spec pages
function cleanSpecContent(content, filename) {
  // Rewrite cross-references so they resolve on the website. The spec markdown
  // uses repo-relative links (e.g. `0004-TypeSystem.md`, `../plans/x.md`) that
  // are valid on GitHub but 404 on the site, where specs live at
  // `/spec/<slug>/`. Without this every inter-spec link is a broken link.

  // Sibling spec pages: `0004-TypeSystem.md[#anchor]` -> `/spec/0004-typesystem/[#anchor]`
  content = content.replace(
    /\]\((\d{4}-[A-Za-z0-9._-]+)\.md(#[^)]*)?\)/g,
    (_m, slug, anchor) => `](/spec/${slug.toLowerCase()}/${anchor || ''})`
  );

  // Plan / design docs aren't published to the site: point them at the repo.
  content = content.replace(
    /\]\((?:\.\.\/)+(?:docs\/)?plans\/([A-Za-z0-9._-]+\.md)(#[^)]*)?\)/g,
    (_m, file, anchor) =>
      `](https://github.com/Nimblesite/osprey/blob/main/docs/plans/${file}${anchor || ''})`
  );

  // Any remaining repo-relative links (source files, shipwright.json, etc.)
  // climb to the repo root from docs/specs/. Send them to the repo on GitHub.
  content = content.replace(
    /\]\((?:\.\.\/){2,}([^)#]+)(#[^)]*)?\)/g,
    (_m, repoPath, anchor) =>
      `](https://github.com/Nimblesite/osprey/blob/main/${repoPath}${anchor || ''})`
  );

  return content.trim();
}

// Helper function to generate URL-friendly slug from filename
function generateSlug(filename) {
  // Remove the .md extension and convert to lowercase
  return filename.replace(/\.md$/, '').toLowerCase();
}

// Helper function to create front matter for spec pages
function createSpecFrontMatter(title, slug, description = '') {
  return `---
layout: page
title: "${title}"
description: "${description}"
date: ${new Date().toISOString().split('T')[0]}
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/${slug}/"
---

`;
}

try {
  console.log('📁 Creating spec directory structure...');

  // Remove previously generated spec pages first. Without this, renamed or
  // renumbered source files leave orphan pages behind (e.g. an old
  // 0015-http.md lingering after HTTP moved to 0014), producing duplicate
  // content. index.md is regenerated separately at the end of this script.
  for (const existing of fs.readdirSync(specDestDir)) {
    if (existing.endsWith('.md') && existing !== 'index.md') {
      fs.unlinkSync(path.join(specDestDir, existing));
    }
  }

  // Read all spec files (skipping meta-docs like README.md)
  const specFiles = fs.readdirSync(specSourceDir)
    .filter(file => file.endsWith('.md') && !SKIP_FILES.has(file));
  const specPages = [];

  // Process each spec file
  for (const file of specFiles) {
    const sourcePath = path.join(specSourceDir, file);
    let content = fs.readFileSync(sourcePath, 'utf8');
    const title = extractTitle(content, file);
    const slug = generateSlug(file);

    let destFile;
    let permalink;

    if (file === 'index.md') {
      // Special handling for index.md - becomes the main spec page
      destFile = 'index.md';
      permalink = '/spec/';
    } else {
      // Clean content for individual pages (remove TOC sections)
      content = cleanSpecContent(content, file);
      destFile = `${slug}.md`;
      permalink = `/spec/${slug}/`;
    }

    const destPath = path.join(specDestDir, destFile);

    // Create appropriate front matter
    const frontMatter = file === 'index.md'
      ? `---
layout: page
title: "Osprey Language Specification"
description: "Complete language specification and syntax reference for the Osprey programming language"
date: ${new Date().toISOString().split('T')[0]}
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/"
---

`
      : createSpecFrontMatter(title, slug, `Osprey Language Specification: ${title}`);

    const contentWithFrontMatter = frontMatter + content;
    fs.writeFileSync(destPath, contentWithFrontMatter, 'utf8');

    // Track pages for index generation (skip index.md itself)
    if (file !== 'index.md') {
      specPages.push({
        file,
        slug,
        title,
        permalink,
        order: file.match(/^(\d+)/) ? parseInt(file.match(/^(\d+)/)[1]) : 999
      });
    }

    console.log(`✅ Processed ${file} → ${destFile} (${permalink}) - "${title}"`);
  }

  // Sort pages by order number
  specPages.sort((a, b) => a.order - b.order);

  // Create/update the spec index with clean table of contents
  const indexPath = path.join(specDestDir, 'index.md');
  let indexContent = fs.readFileSync(indexPath, 'utf8');

  // Remove existing front matter if present
  const frontMatterEnd = indexContent.indexOf('---', 3);
  if (frontMatterEnd > 0) {
    indexContent = indexContent.substring(frontMatterEnd + 3).trim();
  }

  // Create a simple introduction and table of contents only
  const newIndexContent = `# Osprey Language Specification

**Version:** 0.2.0  
**Date:** ${new Date().toISOString().split('T')[0]}  
**Author:** Christian Findlay

## Table of Contents

${specPages.map(page => `${page.order}. [${page.title}](${page.permalink})`).join('\n')}

## About This Specification

This specification defines the complete syntax and semantics of the Osprey programming language. Each section is available as a separate page for easy navigation and reference.

The Osprey language is designed for elegance, safety, and performance, emphasizing:

- **Typed algebraic effects** with lexical handlers; complete static coverage checking remains in progress
- **Named arguments** for multi-parameter functions to improve readability
- **Strong type inference** (Hindley-Milner) to reduce boilerplate while maintaining safety
- **String interpolation** for convenient text formatting
- **Pattern matching** for elegant conditional logic
- **Immutable-by-default** variables and persistent collections
- **Fast HTTP/HTTPS servers and clients** with built-in streaming support
- **C interoperability** via a typed foreign function interface

## Implementation Status

🚧 **NOTE**: The Osprey language and compiler are actively under development. This specification represents the design goals and planned features. Please refer to individual sections for current implementation status.
`;

  // Write updated index with front matter
  const indexFrontMatter = `---
layout: page
title: "Osprey Language Specification"
description: "Complete language specification and syntax reference for the Osprey programming language"
date: ${new Date().toISOString().split('T')[0]}
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/"
---

`;

  fs.writeFileSync(indexPath, indexFrontMatter + newIndexContent, 'utf8');

  console.log('✅ Created clean spec index with table of contents');
  console.log(`📊 Processed ${specFiles.length} spec files:`);
  console.log(`   - Main index: /spec/`);
  specPages.forEach(page => {
    console.log(`   - ${page.order}. ${page.title}: ${page.permalink}`);
  });

} catch (error) {
  console.error('❌ Failed to copy spec files:', error.message);
  process.exit(1);
} 
