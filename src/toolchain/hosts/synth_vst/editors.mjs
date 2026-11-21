// [PRISM-ROUTING-VST-WINDOW] Exercise the actual plug-in controller and Prism window actions.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const [folder, ...selected] = process.argv.slice(2);
assert(folder, 'Supply the configured VST3 folder');
const worker = resolve('target/vst-worker/osprey-vst-worker');
const host = resolve('src/toolchain/hosts/gpui/target/release/osprey-gpui');
const plugins = resolve(folder);
const run = mkdtempSync(resolve('target/pr-validation/vendor-ui-'));
mkdirSync(join(run, '.prism'));
writeFileSync(join(run, '.prism/vst3-folder'), plugins);

function controller(bundle, id, controls) {
    // A program change may reset every other control. Read each write back before the next.
    const endpoints = controls.flatMap(p => [0, 1, p.initial].map(value => ({ id: p.id, value })));
    const commands = endpoints.map(p => 'watch ' + p.id + '=' + p.value);
    const output = execFileSync(worker, ['editor-session', bundle, id], {
        input: [...commands, 'focus', 'close', ''].join('\n'), encoding: 'utf8', timeout: 30000,
    });
    const rows = output.trimEnd().split('\n').map(JSON.parse);
    assert.equal(rows.length, commands.length + 2);
    const state = new Map();
    for (const [index, { id, value }] of endpoints.entries()) {
        const row = rows[index];
        assert(row.window > 0 && row.width > 0 && row.height > 0 && !row.closed);
        for (const changed of row.values) state.set(changed.id, changed.value);
        assert(Math.abs(state.get(id) - value) < 0.00001, 'Parameter ' + id + ' failed at ' + value);
    }
    assert.equal(rows.at(-2).window, rows[0].window);
    assert(rows.at(-1).closed);
}

function inspector(name, id, parameters) {
    const steps = [{ invoke: 'Patch Controls' }, { wait: name }, { type: ['Add component', 'vst:' + id] },
        { wait: name + ' 0' }, { type: ['Module', '0'] }];
    for (const [index, p] of parameters.entries()) {
        if (index && index % 10 === 0) steps.push({ invoke: 'Next parameters' });
        if (p.automatable) steps.push({ invoke: name + ' 0 ' + p.name }, { type: [p.name, p.initial > 0.5 ? '0' : '1'] });
    }
    for (let attempt = 0; attempt < 2; attempt++) steps.push({ invoke: 'Open plug-in window' },
        { wait: 'Plug-in window open.' }, { invoke: 'Focus plug-in window' },
        { invoke: 'Close plug-in window' }, { wait: 'Plug-in window closed.' });
    replay(id, steps, ['--empty']);
}

function replay(id, steps, launch) {
    const journey = join(run, id + '.json');
    writeFileSync(journey, JSON.stringify({ steps }));
    execFileSync(host, ['--self-test', journey, run, '--defer-processes', '--live-processes',
        '--machine', 'src/osprey/packages/signal_flow/signal_flow.machine.json', ...launch, '--vst-worker', worker],
    { timeout: 150000, stdio: 'inherit', env: { ...process.env, HOME: run } });
}

function trackPatch(name, id) {
    const song = join(run, 'arrangement.json');
    copyFileSync('src/osprey/examples/daw/tracks/elise-motif.json', song);
    replay('track-window', [
        { windows: '1' }, { invoke: 'Track Patch' }, { windows: '2' },
        { invoke: 'Patch Controls' }, { wait: name }, { type: ['Add component', 'vst:' + id] },
        { invoke: 'Open plug-in window' }, { wait: 'Plug-in window open.' },
        { invoke: 'Focus plug-in window' }, { invoke: 'Close plug-in window' }, { wait: 'Plug-in window closed.' },
        { invoke: 'Timeline' }, { windows: '2' }, { 'close-child-window': '' }, { windows: '1' },
        { invoke: 'Track Patch' }, { windows: '2' }, { wait: name + ' 6' },
    ], ['--song', song]);
}

const bundles = selected.length ? selected : readdirSync(plugins).filter(file => file.endsWith('.vst3'));
assert(bundles.length, 'No VST3 bundles found');
for (const file of bundles) {
    const bundle = join(plugins, file);
    const scanned = execFileSync(worker, ['scan', bundle], { encoding: 'utf8' }).trimEnd().split('\n');
    for (const { id, name } of scanned.map(JSON.parse)) {
        const { parameters } = JSON.parse(execFileSync(worker, ['describe', bundle, id]));
        const controls = parameters.filter(p => p.automatable);
        controller(bundle, id, controls);
        inspector(name, id, parameters);
        if (file === bundles[0] && id === JSON.parse(scanned[0]).id) trackPatch(name, id);
        console.log(name + ': verified ' + controls.length + ' parameters and two editor window lifecycles');
    }
}
