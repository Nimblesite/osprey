/** A JSON object at a host protocol boundary. */
export function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** A non-negative integer that every JavaScript-backed host can represent exactly. */
export function naturalInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

/** Required native request text, shared by filesystem and package adapters. */
export function requiredString(value: unknown, name: string): string {
  if (typeof value !== "string" || value === "") throw new Error(`Request field ${name} must be a non-empty string`);
  return value;
}

/** A positive request bound; omission is allowed only with an explicit fallback. */
export function boundedInteger(value: unknown, name: string, most: number, fallback?: number): number {
  if (value === undefined && fallback !== undefined) return fallback;
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value <= 0 || value > most) {
    throw new Error(`Request field ${name} must be between 1 and ${most}`);
  }
  return value;
}

/** Native request string lists have one shared count and item-length limit. */
export function stringList(value: unknown, name: string, required = false): string[] {
  if (value === undefined && !required) return [];
  if (!Array.isArray(value) || value.length > 64 || !value.every(item => typeof item === "string" && item.length <= 8192)) {
    throw new Error(`Request field ${name} must be a short array of strings`);
  }
  return value;
}

/** Preserve every failure as readable text at a native host boundary. */
export function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
