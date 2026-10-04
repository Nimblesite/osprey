// Source programs shared by native debugger assertions. Each flavor retains exact line numbers.
export const sourceFixtures = [
    {
      "label": "captured lambda breakpoints expose their own variables",
      "locals": { x: 40, n: 2, sum: 42 },
      "line": 3,
      "prefix": "__closure_fn_",
      "sources": {
        "osp": "fn makeAdder(n) = fn(x) => {\n    let sum = wrapAdd(x, n)\n    sum\n}\nfn main() = {\n    let add = makeAdder(2)\n    print(add(40))\n}\n",
        "ospml": "makeAdder n = \\x =>\n    sum = wrapAdd x n\n    sum\nmain () =\n    add = makeAdder 2\n    print (add 40)\n"
      }
    },
    {
      "label": "extracted GPU kernel breakpoints expose uniforms and locals",
      "locals": { x: 40, n: 2, sum: 42 },
      "line": 5,
      "prefix": "__gpu_kernel_",
      "sources": {
        "osp": "fn main() = {\n    let n = 2\n    let result = gpuMap(toGpu([40]), fn(x) => {\n        let sum = wrapAdd(x, n)\n        sum\n    })\n    print(gpuGet(result, 0) ?: -1)\n}\n",
        "ospml": "main () =\n    n = 2\n    result = gpuMap (toGpu [40]) (\\x =>\n        sum = wrapAdd x n\n        sum)\n    print (gpuGet (result, 0) ?: -1)\n"
      }
    },
    {
      label: "single-expression lambda breakpoints retain return locations",
      locals: { x: 40, n: 2 }, line: 2, prefix: "__closure_fn_",
      sources: {
        osp: "fn makeAdder(n) = fn(x) => {\n    wrapAdd(x, n)\n}\nfn main() = {\n    let add = makeAdder(2)\n    print(add(40))\n}\n",
        ospml: "makeAdder n = \\x =>\n    wrapAdd x n\nmain () =\n    add = makeAdder 2\n    print (add 40)\n",
      },
    },
    ...([ ["int", 42], ["float", 2.5] ] as const).map(([type, value]) => ({
      label: `generic function values retain source names and variables (${type})`,
      locals: { x: value, value }, line: 5, prefix: "identity",
      sources: {
        osp: `type Operation = { run: fn(${type}) -> ${type} }\n\nfn identity(x) = {\n    let value = x\n    value\n}\nfn main() = {\n    let holder = Operation { run: identity }\n    print(holder.run(${value}))\n}\n`,
        ospml: `type Operation =\n    run : ${type} -> ${type}\nidentity x =\n    value = x\n    value\nmain () =\n    holder = Operation(run = identity)\n    print (holder.run ${value})\n`,
      },
    })),
  ];

export const patternFixtures = [
  {
    extension: "osp", outerLine: 12,
    text: "type Choice = Pick(int) | Other\nfn choose(input) = {\n    let value = 100\n    let selected = match input {\n        Pick(value) => {\n            let observed = wrapAdd(value, 1)\n            observed\n        }\n        Other => value\n    }\n    let outside = wrapAdd(value, selected)\n    outside\n}\nprint(choose(Pick(2)))\n",
  },
  {
    extension: "ospml", outerLine: 10,
    text: "type Choice = Pick int | Other\nchoose input =\n    value = 100\n    selected = match input\n        Pick value =>\n            observed = wrapAdd (value, 1)\n            observed\n        Other => value\n    outside = wrapAdd (value, selected)\n    outside\nprint (choose (Pick 2))\n",
  },
];

const defaultBlock = "    let value = 100\n    let selected = {\n        let value = input\n        let observed = wrapAdd(value, 1)\n        observed\n    }\n    let outside = wrapAdd(value, selected)\n    outside\n";
const mlBlock = "    value = 100\n    selected =\n        value = input\n        observed = wrapAdd value 1\n        observed\n    outside = wrapAdd value selected\n    outside\n";

function blockFixture(extension: string, prefix: string, text: string) {
  const lines = text.split("\n");
  return {
    extension, prefix, text,
    label: `nested blocks restore debugger bindings in ${prefix}`,
    bindingLine: lines.findIndex(line => /^(?:let )?value = input$/.test(line.trim())) + 1,
    innerLine: lines.findIndex(line => line.trim() === "observed") + 1,
    outerLine: lines.findIndex(line => ["outside", "resume(outside)", "resume outside"].includes(line.trim())) + 1,
  };
}

const blockFixtures = [
  ...patternFixtures.map(fixture => ({ ...fixture, label: "pattern bindings stay in their debugger arm", innerLine: 7, bindingLine: 6, prefix: "choose" })),
  blockFixture("osp", "choose", `fn choose(input) = {\n${defaultBlock}}\nprint(choose(2))\n`),
  blockFixture("ospml", "choose", `choose input =\n${mlBlock}print (choose 2)\n`),
  blockFixture("osp", "__closure_fn_", `fn main() = {\n let choose = fn(input) => {\n${defaultBlock}}\n print(choose(2))\n}\n`),
  blockFixture("ospml", "__closure_fn_", `main () =\n    choose = \\input =>\n${mlBlock.replace(/^/gm, "    ")}\n    print (choose 2)\n`),
  blockFixture("osp", "__handler_Input_step", `effect Input { step: fn(int) -> int }\nfn main() = {\n handle Input {\n  step input => {\n${defaultBlock}}\n }\n print(perform Input.step(2))\n}\n`),
  blockFixture("ospml", "__handler_Input_step", `effect Input\n    step : int => int\nmain () =\n    handle Input\n        step input =>\n${mlBlock.replace(/^/gm, "        ")}\n    print (perform Input.step 2)\n`),
].flatMap(fixture => fixture.prefix === "__handler_Input_step" ? [fixture, blockFixture(
  fixture.extension, "__resume_arm_Input_step", fixture.text
    .replace("step:", "control step:").replace("step :", "control step :")
    .replace(/^(\s*)outside$/m, fixture.extension === "osp" ? "$1resume(outside)" : "$1resume outside"),
)] : [fixture]);

function initializerFixture(extension: string, mutable: boolean) {
  const declaration = mutable ? "mut" : "let";
  const text = extension === "osp"
    ? `effect Read { get: fn() -> int }\nfn choose(input) = {\n    let value = 100\n    let selected = {\n        ${declaration} value = {\n            let observed = wrapAdd(value, input)\n            observed\n        }\n        ${mutable ? "handle Read { get => value }\n        perform Read.get()" : "wrapAdd(value, 1)"}\n    }\n    let outside = wrapAdd(value, selected)\n    outside\n}\nprint(choose(2))\n`
    : `effect Read\n    get : Unit => int\nchoose input =\n    value = 100\n    selected =\n        ${mutable ? "mut " : ""}value =\n            observed = wrapAdd value input\n            observed\n        ${mutable ? "handle Read\n            get => value\n        perform Read.get ()" : "wrapAdd value 1"}\n    outside = wrapAdd value selected\n    outside\nprint (choose 2)\n`;
  return {
    ...blockFixture(extension, "choose", text),
    label: `${mutable ? "cell" : "immutable"} initializer keeps the enclosing debugger binding`,
    bindingLine: text.split("\n").findIndex(line => line.trim().endsWith("value = 100")) + 1,
    innerLocals: { observed: 102 }, innerValue: 100, selected: mutable ? 102 : 103, uniqueValue: true,
  };
}

export const lexicalFixtures = [
  ...blockFixtures.map(fixture => ({ ...fixture, innerLocals: { observed: 3 }, innerValue: 2, selected: 3, uniqueValue: false })),
  ...["osp", "ospml"].flatMap(extension => [false, true].map(mutable => initializerFixture(extension, mutable))),
];
