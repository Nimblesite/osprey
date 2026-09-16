import * as path from "path";
import Mocha from "mocha";

export function run(): Promise<void> {
  const mocha = new Mocha({ ui: "tdd", timeout: 30000, color: true });
  for (const name of ["signatures", "unused"]) {
    mocha.addFile(path.join(__dirname, `${name}.test.js`));
  }
  return new Promise((resolve, reject) => {
    mocha.run((failures) => failures === 0 ? resolve() : reject(new Error(`${failures} installed VSIX tests failed`)))
      .on("fail", (test, error) => process.stderr.write(`\n${test.fullTitle()}\n${error.stack}\n`));
  });
}
