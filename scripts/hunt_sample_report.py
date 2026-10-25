"""Plain review output for the bounded corpus qualifier [SYMEX-CORPUS-QUICK]."""
from collections import Counter
from html import escape
import json
from pathlib import Path

from hunt_sample import qualifies


def outcome(record):
    if record.get('expectedCrash'):
        return 'Expected crash and benign input verified' if record.get('assertionsPassed') else 'Assertion failed: '+record.get('assertionError', 'unknown failure')
    if record['timedOut']:
        return 'Killed at 10-second deadline'
    if record['exitCode'] < 0:
        return 'Hunt process crashed'
    if record['qualifies']:
        return 'Quick candidate'
    findings = record['report'].get('findings', [])
    if any(f.get('replay') == 'reproduced' for f in findings):
        return 'Expected planted crash reproduced' if record.get('planted') else 'Unexpected native crash reproduced'
    if findings:
        return 'Engine limitation / unconfirmed finding'
    if record['exitCode'] != 0:
        return 'Compilation / tool error'
    return 'Incomplete / invalid analysis'


def links(record, out):
    folder = Path(record['directory'])
    files = [('HTML', 'engine/report.html'), ('JSON', 'engine/report.json'),
             ('Output', 'stdout.log'), ('Log', 'stderr.log'), ('Command', 'command.json'),
             ('Benign output', 'benign/stdout.log')]
    return ' · '.join(f'<a href="{escape((folder/name).relative_to(out).as_posix())}">{label}</a>'
                      for label, name in files if (folder/name).is_file())


def run_status(record):
    report = record.get('report') or {}
    if record.get('timedOut'):
        return 'Timed out'
    if report.get('incomplete'):
        return 'Incomplete'
    if record.get('exitCode', 1) < 0 or (record.get('exitCode', 1) != 0 and not report.get('findings')):
        return 'Failed'
    return 'Completed' if report.get('explored', {}).get('paths', 0) > 0 else 'Not checked'


def issues(record):
    findings = (record.get('report') or {}).get('findings', [])
    if any(f.get('replay') == 'reproduced' for f in findings):
        return 'Expected planted crash' if record.get('planted') or record.get('expectedCrash') else 'BUG: crash reproduced'
    if any(f.get('class') != 'unmodelled' for f in findings):
        return 'Potential bug: not reproduced'
    return 'None found' if qualifies(record) else 'Not fully checked'


def row(record, out):
    report = record.get('report') or {}
    explored = report.get('explored', {})
    values = [record['program'], run_status(record), issues(record)]
    cells = ''.join(f'<td>{escape(value)}</td>' for value in values)
    detail = f"{record['wallSeconds']:.3f}s · Paths explored: {explored.get('paths', '—')} · Normal exits: {explored.get('completed', '—')} · Unfinished: {len(report.get('incomplete', []))}"
    return f'<tr>{cells}<td><details><summary>Details</summary><p>{escape(detail)}</p>{links(record, out)}</details></td></tr>'


def table(records, out):
    headings = ['Program', 'Run', 'Issues', 'Details']
    head = ''.join(f'<th scope="col">{heading}</th>' for heading in headings)
    return f'<div class="table-wrap"><table><thead><tr>{head}</tr></thead><tbody>{"".join(row(r, out) for r in records)}</tbody></table></div>'


def safe_manifest(out, quick):
    selection = out/'selection.json'
    metadata = json.loads(selection.read_text()) if selection.is_file() else {}
    limits = {'wallTimeSeconds': metadata.get('wallTime', 10),
              'engineTimeSeconds': metadata.get('engineBudget', 8),
              'inputBytesPerStream': metadata.get('inputBytes', 2)}
    manifest = {'version': 1, 'limits': limits, 'programs': sorted({r['program'] for r in quick}),
                'toolchain': metadata.get('fingerprints', {})}
    (out/'safe-runs.json').write_text(json.dumps(manifest, indent=2)+'\n')


def write(out, records, proof=False):
    accepted = [bool(r.get('assertionsPassed')) if proof else qualifies(r) for r in records]
    quick = [r for r, ok in zip(records, accepted) if ok]
    excluded = [r for r, ok in zip(records, accepted) if not ok]
    if not proof:
        safe_manifest(out, quick)
        (out/'quick-corpus.txt').write_text(''.join(record['program']+'\n' for record in quick))
    (out/'excluded.json').write_text(json.dumps(excluded, indent=2)+'\n')
    counts = Counter(outcome(record) for record in records)
    (out/'SUMMARY.txt').write_text('\n'.join(f'{key}: {value}' for key, value in sorted(counts.items()))+'\n')
    tokens = (Path(__file__).resolve().parent.parent/'website/src/css/variables.css').read_text()
    (out/'index.html').write_text(document(out, quick, excluded, counts, tokens, proof))


def document(out, quick, excluded, counts, tokens, proof):
    records = quick + excluded
    bugs = sum(issues(r) == 'BUG: crash reproduced' for r in records)
    title = f'{len(quick)}/{len(records)} planted crash tests passed' if proof else 'Hunt results'
    summary = f'{len(records)} programs · {len(quick)} clean runs · {bugs} unexpected crashes reproduced'
    if proof:
        summary = f'{len(records)} programs · {len(quick)} passed · {len(excluded)} failed'
    manifest = '' if proof else '<a href="safe-runs.json">Safe runs JSON for CI</a> · '
    ordered = sorted(records, key=lambda r: (issues(r) != 'BUG: crash reproduced', r['program']))
    return f'''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'">
<title>Osprey Hunt · {title}</title><style>{tokens}{STYLE}</style></head><body><main>
<header>Osprey Hunt</header><h1>{title}</h1><p class="lead">{summary}</p>
<nav>{manifest}<a href="results.json">All results JSON</a></nav>
{table(ordered, out)}</main></body></html>'''


STYLE = '''*{box-sizing:border-box}body{margin:0;background:var(--surface);color:var(--on-surface);font:16px/1.65 var(--font-sans)}main{max-width:1400px;padding:40px 24px 64px;margin:auto}header{color:var(--primary-container);border-bottom:1px solid var(--outline-variant);padding-bottom:24px}h1{font-size:clamp(32px,5vw,52px);line-height:1.15;letter-spacing:-1px;font-weight:550}h2{font-weight:500;margin-top:32px}.lead,footer{color:var(--on-surface-variant)}a{color:var(--primary);text-underline-offset:3px}a:focus-visible{outline:2px solid var(--primary-container);outline-offset:3px}.card,.table-wrap{border:1px solid var(--outline-variant);border-radius:12px;background:var(--surface-container);margin:24px 0;padding:24px}.table-wrap{overflow-x:auto;padding:0}table{border-collapse:collapse;width:100%;font-size:13px}th,td{text-align:left;padding:14px 16px;vertical-align:top;border-bottom:1px solid var(--outline-variant)}td:first-child{font:12px/1.6 var(--font-mono);overflow-wrap:anywhere;min-width:280px}td:last-child{min-width:180px}footer{margin-top:32px;font-size:13px}@media(max-width:640px){main{padding:24px 16px}.card{padding:20px}}'''
