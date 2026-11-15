/**
 * [TYPE-RECORD-CONTEXT] The TypeScript twin of contextual_records.test.osp:
 * an object literal needs no `satisfies` where a declared type gives its
 * record. [FLAVOR-TS-UNIONS]
 */
interface Point {
  readonly x: int;
  readonly y: int;
}

interface Segment {
  readonly from: Point;
  readonly to: Point;
  readonly label: string;
}

function sum(p: Point) {
  return satAdd(p.x, p.y);
}

function origin(): Point {
  return { x: 0, y: 0 };
}

function longest(points: Point[]) {
  return points.reduce((best, p) => (sum(p) > best ? sum(p) : best), 0);
}

function contextualCase() {
  const annotated: Point = { x: 1, y: 2 };
  const segment: Segment = { from: { x: 3, y: 4 }, to: origin(), label: "diagonal" };
  const many: Point[] = [{ x: 5, y: 6 }, { y: 9, x: 7 }];
  return checkAll("object literals take their declared record type", [
    sum(annotated) === 3,
    sum({ x: 10, y: 20 }) === 30,
    sum(origin()) === 0,
    sum(segment.from) === 7,
    sum(segment.to) === 0,
    segment.label === "diagonal",
    longest(many) === 16,
    structuralEquals(annotated, { x: 1, y: 2 } satisfies Point),
    `${annotated.x},${annotated.y}` === "1,2",
  ]);
}

test("object literals take the record type their context declares", exactly(contextualCase));
