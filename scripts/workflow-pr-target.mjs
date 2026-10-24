// Explicit non-main branch lists do not report checks on main PRs.
// Unknown filters and glob patterns remain in scope, so the guard fails closed.
const trigger = (text) => text.match(/^  pull_request:(.*(?:\n(?: {4,}[^\n]*|[ \t]*))*)/m)?.[1]
const branchList = (block) => {
  const filter = block.match(/^    branches:\s*(\[[^\n]*\]|\n(?:      - [^\n]*\n?)*)/m)?.[1]
  return filter?.replace(/[\[\]"']/g, '').split(/,|\n/).map((value) => value.trim().replace(/^- /, '')).filter(Boolean)
}

export const targetsMainPullRequests = (text) => {
  const block = trigger(text.replace(/^\s*#.*$/gm, ''))
  if (block === undefined) return /^on:.*\bpull_request\b/m.test(text)
  const branches = branchList(block)
  return !branches?.length || branches.some((name) => name === 'main' || /[*?+!\[]/.test(name))
}
