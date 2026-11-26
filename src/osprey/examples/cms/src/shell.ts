// osprey: namespace=cms
// osprey: module=Shell

/**
 * Commands run in the site's working tree. Every argument that came from a
 * request has passed [CMS-PATH-SAFE] first, so no quote, space or `$` reaches
 * the shell from outside. Output goes to a file under `.git/`, because the
 * process callback cannot hand text back to its caller. [CMS-API]
 */

/** Where a command's output waits to be read. */
const OUTPUT = ".git/cms-out";

function ignored(_processID: int, _eventType: int, _data: string) {
  return;
}

/** The exit code of `command`, or -1 when it could not start. */
function exitOf(command) {
  const started = spawnProcess(command, ignored);
  switch (started.kind) {
    case "success": {
      const code = awaitProcess(started.value);
      cleanupProcess(started.value);
      return code;
    }
    case "error":
      return -1;
  }
}

/** Run `command` inside `repo`: its trimmed output on exit 0, otherwise the output as the reason. */
export function run(repo: string, command: string): Result<string, string> {
  const code = exitOf(`cd '${repo}' && (${command}) > ${OUTPUT} 2>&1`);
  const output = trim(unwrapOr(readFile(`${repo}/${OUTPUT}`), ""));
  if (code === 0) {
    return Success(output);
  }
  return Error(output === "" ? `command failed with exit code ${code}` : output);
}

/** Whether `command` exits 0 inside `repo`, for checks whose output does not matter. */
export function succeeds(repo: string, command: string) {
  const outcome = run(repo, command);
  switch (outcome.kind) {
    case "success":
      return true;
    case "error":
      return false;
  }
}

/** Whether `text` can sit inside single quotes on one shell line. */
export function quotable(text: string) {
  return !contains(text, "'") && !contains(text, "\n") && !contains(text, "\r");
}
