// Shared by grammar.js and its rule modules: the precedence table and the
// one-or-more separated list.

const PREC = {
  ternary: 1,
  or: 2,
  and: 3,
  compare: 4,
  add: 5,
  mul: 6,
  pipe: 7,
  unary: 8,
  call: 9,
  member: 10,
};

function sep1(separator, rule) {
  return seq(rule, repeat(seq(separator, rule)));
}

module.exports = { PREC, sep1 };
