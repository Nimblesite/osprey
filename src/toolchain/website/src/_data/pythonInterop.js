const { readFileSync } = require("node:fs");
const path = require("node:path");
const read = file => readFileSync(path.resolve(__dirname, "../../../../osprey/examples/python", file), "utf8").trimEnd();
module.exports = () => ({ osprey: read("interop.osp"), flavor: read("interop.py") });
