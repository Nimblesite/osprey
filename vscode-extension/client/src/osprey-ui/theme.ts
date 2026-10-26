// Implements [UI-THEME]: the Osprey application owns its palette; the host
// emits only sanitized CSS custom properties. Surfaces and text still follow
// the editor's theme, so the app's navy/cyan identity never fights the IDE.

import { record } from "./protocol";

interface ThemeToken {
  readonly name: string;
  readonly light: string;
  readonly dark: string;
}


function colour(value: unknown): value is string {
  return typeof value === "string" && /^#[0-9a-fA-F]{6}$/.test(value);
}

function tokenName(value: unknown): value is string {
  return typeof value === "string" && /^[a-z][a-z0-9-]{0,31}$/.test(value);
}

function tokens(theme: unknown): ThemeToken[] {
  if (!record(theme) || !Array.isArray(theme.tokens)) return [];
  return theme.tokens.flatMap((item) => {
    if (!record(item) || !tokenName(item.name) || !colour(item.light) || !colour(item.dark)) return [];
    return [{ name: item.name, light: item.light, dark: item.dark }];
  });
}

/** Sanitized brand custom properties for the application's theme; empty when it supplies none. */
export function themeStyle(theme: unknown): string {
  const declarations = tokens(theme).map((token) =>
    `--osp-brand-${token.name}: light-dark(${token.light}, ${token.dark});`);
  return declarations.length === 0 ? "" : `<style>:root{${declarations.join("")}}</style>`;
}
