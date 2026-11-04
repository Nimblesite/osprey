import { naturalInteger, record } from "./wire";

export interface BoardStroke {
  readonly points: readonly (readonly [number, number])[];
  readonly width: number;
  readonly at: number;
}

/** The bounded stroke payload of a typed Native Board binding. */
export function boardStroke(value: unknown): value is BoardStroke {
  return record(value) && naturalInteger(value.width) && value.width > 0 && naturalInteger(value.at)
    && Array.isArray(value.points) && value.points.length > 0 && value.points.length <= 8192
    && value.points.every((point) => Array.isArray(point) && point.length === 2 && point.every(naturalInteger));
}

/** A bounded list of selected stroke indices, including an empty selection. */
export function boardSelection(value: unknown): value is readonly number[] {
  return Array.isArray(value) && value.length <= 10000 && value.every(naturalInteger);
}
