// Verbatim excerpt of the package manager's dispatch tests for the effects guide.
const { readFileSync } = require("node:fs");
const path = require("node:path");
const file = "src/osprey/packages/manager/cli/tests/src/dispatch_tests.osp";
const source = readFileSync(path.resolve(__dirname, "../../../../..", file), "utf8");
const block = name => {
  const text = source.match(new RegExp(`^    fn ${name}\\(\\) = [\\s\\S]*?^    }\\n`, "m"))?.[0];
  if (!text) throw new Error(`effects guide excerpt moved: ${name} in ${file}`);
  return text.replace(/^    /gm, "").trimEnd();
};
const call = source.match(/^\s*test\("dispatch: doctor on a broken host", doctorBroken\)$/m)?.[0].trim();
if (!call) throw new Error(`effects guide excerpt moved: doctorBroken test in ${file}`);
module.exports = { file, excerpt: [block("healthyFiles"), block("brokenFiles"), block("brokenProcess"), block("doctorBroken"), call].join("\n\n") };
