// Implements [UI-THEME]: the Osprey application owns its palette; the host
// emits sanitized semantic CSS custom properties. The editor supplies fallback
// colours only when the application does not supply a token.

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
  return typeof value === "string" && /^[a-z][a-zA-Z0-9-]{0,31}$/.test(value);
}

function tokens(theme: unknown): ThemeToken[] {
  if (!record(theme) || !Array.isArray(theme.tokens)) return [];
  return theme.tokens.flatMap((item) => {
    if (!record(item) || !tokenName(item.name) || !colour(item.light) || !colour(item.dark)) return [];
    return [{ name: item.name, light: item.light, dark: item.dark }];
  });
}

const ALIASES: Readonly<Record<string, readonly string[]>> = {
  primary: ["text"], secondary: ["muted"], outline: ["border"], surface: ["surface", "raised"], accent: ["accent", "accent-strong"],
};

function tokenDeclarations(token: ThemeToken): string[] {
  const name = token.name.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`);
  const aliases = Object.prototype.hasOwnProperty.call(ALIASES, name) ? ALIASES[name] : [name];
  const colour = `light-dark(${token.light}, ${token.dark})`;
  return [`--osp-brand-${token.name}: ${colour};`, ...aliases.map((alias) => `--osp-${alias}: ${colour};`)];
}

function duration(theme: unknown, key: string, property: string): string[] {
  if (!record(theme)) return [];
  const value = theme[key];
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 && value <= 10000 ? [`--osp-${property}:${value}ms;`] : [];
}

/** Sanitized semantic and brand properties for the application's theme; empty when it supplies none. */
export function themeStyle(theme: unknown): string {
  const declarations = [...tokens(theme).flatMap(tokenDeclarations), ...duration(theme, "feedback", "motion"), ...duration(theme, "settle", "settle")];
  return declarations.length === 0 ? "" : `<style>:root{${declarations.join("")}}</style>`;
}
