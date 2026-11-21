// [PRISM-ROUTING-VST-WINDOW] Exercise the actual plug-in controller and Prism window actions.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const [folder, ...selected] = process.argv.slice(2);
assert(folder, 'Supply the configured VST3 folder');
const worker = resolve('target/vst-worker/osprey-vst-worker');
const host = resolve('src/toolchain/hosts/gpui/target/release/osprey-gpui');
const plugins = resolve(folder);
const output = resolve('target/pr-validation');
mkdirSync(output, { recursive: true });
const run = mkdtempSync(join(output, 'vendor-ui-'));
mkdirSync(join(run, '.prism'));
writeFileSync(join(run, '.prism/vst3-folder'), plugins);

const loggedWorker = join(run, 'worker');
writeFileSync(loggedWorker, `#!/bin/bash
set -eo pipefail
if [[ "$1" == editor-session ]]; then
    exec "$VST_WORKER" "$@" < <(tee -a "$PRISM_WINDOW_COMMANDS")
else
    exec "$VST_WORKER" "$@"
fi
`, { mode: 0o755 });

// These vendor fixtures support opaque state. Dragonfly's dry mix also has a known audio response.
function verifyVendorState(bundle, id, name, inputs, controls) {
    const dragonfly = /^Dragonfly (Early Reflections|Hall Reverb|Plate Reverb|Room Reverb)$/.test(name);
    const label = dragonfly ? 'Dry Level' : { 'TAL-NoiseMaker': 'Filter Cutoff', CHOWTapeModel: 'Input Gain' }[name];
    if (typeof label !== 'string') return;
    const parameter = controls.find(control => control.name === label);
    assert(parameter, name + ' is missing its state test parameter ' + label);
    const args = [resolve('src/toolchain/hosts/synth_vst/state-session.mjs'), worker, bundle, id, String(inputs), String(parameter.id)];
    if (dragonfly) args.push(String(parameter.initial));
    execFileSync(process.execPath, args, { stdio: 'inherit', timeout: 30000 });
}

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

function inspector(name, id, parameters, unresponsive) {
    const steps = [{ invoke: 'Patch Controls' }, { wait: name }, { type: ['Add component', 'vst:' + id] },
        { wait: name + ' 0' }, { type: ['Module', '0'] }];
    if (unresponsive) steps.push({ invoke: 'Open plug-in window' }, { wait: 'Opening plug-in window…' },
        { gone: 'Opening plug-in window…' }, { wait: 'Open plug-in window' });
    for (const [index, p] of parameters.entries()) {
        if (index && index % 10 === 0) steps.push({ invoke: 'Next parameters' });
        if (p.automatable) steps.push({ invoke: name + ' 0 ' + p.name }, { type: [p.name, p.initial > 0.5 ? '0' : '1'] });
    }
    if (unresponsive) {
        replay(id, steps, ['--empty']);
        return;
    }
    const lastPage = parameters.slice(Math.floor((parameters.length - 1) / 10) * 10);
    const live = lastPage.find(p => p.automatable);
    for (let attempt = 0; attempt < 2; attempt++) {
        steps.push({ invoke: 'Open plug-in window' }, { wait: 'Plug-in window open.' }, { invoke: 'Focus plug-in window' }, { frames: '12' });
        if (!attempt && live) steps.push({ invoke: name + ' 0 ' + live.name },
            { type: [live.name, live.initial > 0.5 ? '1' : '0'] }, { frames: '8' });
        steps.push({ invoke: 'Close plug-in window' }, { wait: 'Plug-in window closed.' });
    }
    const commands = replay(id, steps, ['--empty']);
    assertWindowCycles(commands, 2);
    const first = commands.find(line => line.startsWith('watch '));
    assert(first, 'Prism must send the edited patch to its native window');
    const settings = line => new Map(line.split(' ').slice(1).map(pair => pair.split('=').map(Number)));
    const submitted = settings(first);
    if (live) assert(commands.slice(1).some(line => settings(line).get(live.id) === (live.initial > 0.5 ? 1 : 0)),
        'The open editor must receive a subsequent change to ' + live.name);
    for (const p of parameters.filter(p => p.automatable)) {
        assert.equal(submitted.get(p.id), p.initial > 0.5 ? 0 : 1, 'Inspector value for ' + p.name);
    }
}

function assertWindowCycles(commands, count) {
    const actions = commands.map(line => line.split(' ')[0]).filter(action => action !== 'watch');
    assert.deepEqual(actions, Array.from({ length: count }, () => ['focus', 'close']).flat());
}

function replay(id, steps, launch) {
    const journey = join(run, id + '.json');
    writeFileSync(journey, JSON.stringify({ steps }));
    const commandLog = join(run, id + '.commands');
    execFileSync(host, ['--self-test', journey, run, '--defer-processes', '--live-processes',
        '--machine', 'src/osprey/packages/signal_flow/signal_flow.machine.json', ...launch, '--vst-worker', loggedWorker],
    { timeout: 150000, stdio: 'inherit', env: { ...process.env, HOME: run, VST_WORKER: worker, PRISM_WINDOW_COMMANDS: commandLog } });
    return readFileSync(commandLog, 'utf8').trimEnd().split('\n');
}

function trackPatch(name, id) {
    const song = join(run, 'arrangement.json');
    copyFileSync('src/osprey/examples/daw/tracks/elise-motif.json', song);
    const commands = replay('track-window', [
        { windows: '1' }, { invoke: 'Track Patch' }, { windows: '2' },
        { invoke: 'Patch Controls' }, { wait: name }, { type: ['Add component', 'vst:' + id] },
        { invoke: 'Open plug-in window' }, { wait: 'Plug-in window open.' },
        { invoke: 'Focus plug-in window' }, { frames: '12' }, { invoke: 'Close plug-in window' }, { wait: 'Plug-in window closed.' },
        { invoke: 'Timeline' }, { windows: '2' }, { 'close-child-window': '' }, { windows: '1' }, { gone: 'Canvas ' + name + ' 6' },
        { invoke: 'Track Patch' }, { windows: '2' }, { wait: name + ' 6' },
    ], ['--song', song]);
    assertWindowCycles(commands, 1);
}

const unresponsive = selected[0] === '--unresponsive-editor';
if (unresponsive) selected.shift();
assert(!unresponsive || selected.length === 1, 'Specify exactly one unresponsive VST3 bundle');
const bundles = selected.length ? selected : readdirSync(plugins).filter(file => file.endsWith('.vst3'));
assert(bundles.length, 'No VST3 bundles found');
for (const file of bundles) {
    const bundle = join(plugins, file);
    const scanned = execFileSync(worker, ['scan', bundle], { encoding: 'utf8', timeout: 15000 }).trimEnd().split('\n');
    for (const { id, name } of scanned.map(JSON.parse)) {
        const { inputs, parameters } = JSON.parse(execFileSync(worker, ['describe', bundle, id], { timeout: 15000 }));
        const controls = parameters.filter(p => p.automatable);
        verifyVendorState(bundle, id, name, inputs, controls);
        if (!unresponsive) controller(bundle, id, controls);
        inspector(name, id, parameters, unresponsive);
        if (unresponsive) {
            console.log(name + ': Prism recovered from the unresponsive editor and accepted edits to ' + controls.length + ' parameters');
            continue;
        }
        if (file === bundles[0] && id === JSON.parse(scanned[0]).id) trackPatch(name, id);
        console.log(name + ': verified ' + controls.length + ' parameters and two editor window lifecycles');
    }
}
