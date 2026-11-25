// #401: a string read out of a map on an unannotated record parameter is compared as a string,
// not as a number, once it is handed to another unannotated function.
interface Request { readonly query: Map<string, string>; readonly name: string }

function known(req, key) {
  return key === "" ? `${req.name}: empty key` : key === "secret" ? `${req.name}: ok` : `${req.name}: bad ${key}`;
}

function checked(req) {
  const key = mapGet(req.query, "api-key");
  switch (key.kind) {
    case "error":
      return `${req.name}: no key`;
    case "success":
      return known(req, key.value);
  }
}

function asked(name: string, pairs: Map<string, string>): Request { return { query: pairs, name: name }; }

print(`${checked(asked("a", mapSet(Map(), "api-key", "secret")))} | ${checked(asked("b", mapSet(Map(), "api-key", "")))} | ${checked(asked("c", mapSet(Map(), "other", "x")))} | ${checked(asked("d", mapSet(Map(), "api-key", "nope")))}`);
