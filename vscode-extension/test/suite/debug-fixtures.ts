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
