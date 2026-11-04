const { readFileSync } = require("node:fs");
const path = require("node:path");
const root = path.resolve(__dirname, "../../../../osprey/examples/gpt2/python");
const read = file => readFileSync(path.join(root, file), "utf8");
const required = (source, pattern) => {
  const text = source.match(pattern)?.[0];
  if (!text) throw new Error(`GPT-2 excerpt moved: ${pattern}`);
  return text.trimEnd();
};
const method = (source, name) => required(source, new RegExp(`^    def ${name}\\([^\\n]+\\n(?:[ \\t]{8}[^\\n]+\\n)+`, "m"));
const handler = (source, name) => `${required(source, new RegExp(`^@static_handler[^\\n]+\\nclass ${name}:`, "m"))}\n${method(source, "mat_mul")}\n\n${method(source, "device")}`;
module.exports = () => {
  const gpu = read("gpu.py");
  const tensor = read("backends.py");
  const native = read("../model/backends.osp");
  const nativeGpu = read("../model/gpu.osp");
  return {
    nativeTensor: required(native, /^    export static effect Tensor<B> \{[\s\S]*?^    \}/m).replace(/^    /gm, ""),
    nativeGpuProduct: nativeGpu.split("\n").filter(line => /matMul a b|load path name/.test(line)).map(line => line.trim()).join("\n"),
    product: required(read("model.py"), /^def product\([^\n]+\n(?:[ \t]+[^\n]+\n)+/m),
    tensor: `@static_effect\nclass Tensor[B]:\n${tensor.split("\n").filter(line => /^    def (build|get|mat_mul|load|device)\(/.test(line)).join("\n")}`,
    cpu: handler(read("cpu.py"), "on_lists"),
    gpu: `${required(gpu, /^def answered\([^\n]+\n(?:[ \t]+[^\n]+\n)+/m)}\n\n\n${handler(gpu, "on_gpu")}`,
  };
};
