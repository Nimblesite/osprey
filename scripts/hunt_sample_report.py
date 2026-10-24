"""Plain review output for the bounded corpus qualifier [SYMEX-CORPUS-QUICK]."""
from collections import Counter
from html import escape
import json
from pathlib import Path


def outcome(record):
    if record.get('expectedCrash') and record.get('assertionsPassed'):
        return 'Expected crash and benign input verified'
    if record['timedOut']:
        return 'Killed at 10-second deadline'
    if record['exitCode'] < 0:
        return 'Hunt process crashed'
    if record['qualifies']:
        return 'Quick candidate'
    findings = record['report'].get('findings', [])
    if any(f.get('replay') == 'reproduced' for f in findings):
        return 'Native crash reproduced'
    if findings:
        return 'Engine limitation / unconfirmed finding'
    if record['exitCode'] != 0:
        return 'Compilation / tool error'
    return 'Incomplete / invalid analysis'


def links(record, out):
    folder = Path(record['directory'])
    files = [('HTML', 'engine/report.html'), ('JSON', 'engine/report.json'),
             ('Output', 'stdout.log'), ('Log', 'stderr.log'), ('Command', 'command.json')]
    return ' · '.join(f'<a href="{escape((folder/name).relative_to(out).as_posix())}">{label}</a>'
                      for label, name in files if (folder/name).is_file())


def row(record, out):
    explored = record['report'].get('explored', {})
    values = [record['program'], outcome(record), f"{record['wallSeconds']:.3f}s",
              str(explored.get('paths', '—')), str(explored.get('completed', '—')),
              str(len(record['report'].get('incomplete', []))) if explored else '—']
    cells = ''.join(f'<td>{escape(value)}</td>' for value in values)
    return f'<tr>{cells}<td>{links(record, out)}</td></tr>'


def table(records, out):
    headings = ['Program', 'Result', 'Wall time', 'Paths explored', 'Normal exits', 'Unfinished', 'Review']
    head = ''.join(f'<th scope="col">{heading}</th>' for heading in headings)
    return f'<div class="table-wrap"><table><thead><tr>{head}</tr></thead><tbody>{"".join(row(r, out) for r in records)}</tbody></table></div>'


def write(out, records, proof=False):
    field = 'assertionsPassed' if proof else 'qualifies'
    quick = [record for record in records if record[field]]
    excluded = [record for record in records if not record[field]]
    if not proof:
        (out/'quick-corpus.txt').write_text(''.join(record['program']+'\n' for record in quick))
    (out/'excluded.json').write_text(json.dumps(excluded, indent=2)+'\n')
    counts = Counter(outcome(record) for record in records)
    (out/'SUMMARY.txt').write_text('\n'.join(f'{key}: {value}' for key, value in sorted(counts.items()))+'\n')
    tokens = (Path(__file__).resolve().parent.parent/'website/src/css/variables.css').read_text()
    (out/'index.html').write_text(document(out, quick, excluded, counts, tokens, proof))


def document(out, quick, excluded, counts, tokens, proof):
    summary = ' · '.join(f'{escape(key)}: {count}' for key, count in sorted(counts.items()))
    return f'''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'">
<title>Osprey Hunt · {title}</title><style>{tokens}{STYLE}</style></head><body><main>
<header>Osprey Hunt / Verification</header><h1>{title}</h1>
<p class="lead">Every invocation had a hard 10-second wall-clock deadline, including compilation, symbolic execution and native replay. Timed-out process groups were killed.</p>
<p>{summary}</p><nav><a href="#quick">{qualified}</a> · <a href="#excluded">{rejected}</a> · {manifest}<a href="results.json">All results JSON</a> · <a href="results.json">Run details</a></nav>
<section class="card"><h2>What is asserted</h2><p>{policy}</p></section>
<section class="card"><h2>How to read the counts</h2><p>Paths are different possible executions, not repeated test attempts. A path that reaches a detected crash does not count as a normal exit. For the planted crash example: four paths explored, three normal exits, one detected crash, zero unfinished. That is a successful detection.</p><p>Findings are deduplicated; their count need not equal the number of failing paths. Unfinished paths mean a time, solver or resource limit prevented analysis. An unsupported operation is an engine limitation, not a demonstrated program bug. Native replay must say reproduced before a finding counts as a confirmed native crash.</p></section>
<h2 id="quick">{qualified}</h2>{table(quick, out)}<h2 id="excluded">{rejected}</h2><p>Expected planted crashes are also excluded from the safe corpus. Open their reports to inspect the detected input and native replay.</p>{table(excluded, out)}
<footer>{footer}</footer></main></body></html>'''


STYLE = '''*{box-sizing:border-box}body{margin:0;background:var(--surface);color:var(--on-surface);font:16px/1.65 var(--font-sans)}main{max-width:1400px;padding:40px 24px 64px;margin:auto}header{color:var(--primary-container);border-bottom:1px solid var(--outline-variant);padding-bottom:24px}h1{font-size:clamp(32px,5vw,52px);line-height:1.15;letter-spacing:-1px;font-weight:550}h2{font-weight:500;margin-top:32px}.lead,footer{color:var(--on-surface-variant)}a{color:var(--primary);text-underline-offset:3px}a:focus-visible{outline:2px solid var(--primary-container);outline-offset:3px}.card,.table-wrap{border:1px solid var(--outline-variant);border-radius:12px;background:var(--surface-container);margin:24px 0;padding:24px}.table-wrap{overflow-x:auto;padding:0}table{border-collapse:collapse;width:100%;font-size:13px}th,td{text-align:left;padding:14px 16px;vertical-align:top;border-bottom:1px solid var(--outline-variant)}td:first-child{font:12px/1.6 var(--font-mono);overflow-wrap:anywhere;min-width:280px}td:last-child{min-width:180px}footer{margin-top:32px;font-size:13px}@media(max-width:640px){main{padding:24px 16px}.card{padding:20px}}'''
