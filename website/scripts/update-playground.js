#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

// The playground editor is seeded from the same migrated assertion suite in both flavors.
const SHOWCASES = {
    osp: path.join(__dirname, '../../tests/regressions/basics/osprey_mega_showcase.test.osp'),
    ospml: path.join(__dirname, '../../tests/regressions/basics/osprey_mega_showcase.test.ospml'),
};
const PLAYGROUND_PATH = path.join(__dirname, '../src/playground/index.md');

// Each showcase is embedded inside a JS template literal (`key: \`...\``). Any
// backslash, backtick, or `${` in the Osprey source must be escaped or the
// browser evaluates it as JS — an unescaped `${expr}` (Osprey string
// interpolation) throws a ReferenceError that aborts the script and leaves
// Monaco uninitialised, i.e. a blank playground. Order matters: backslashes
// first so the escapes we add below are not themselves re-escaped.
function escapeForTemplateLiteral(str) {
    return str
        .replace(/\\/g, '\\\\')
        .replace(/`/g, '\\`')
        .replace(/\$\{/g, '\\${');
}

// Replace the template literal that follows `<flavor>: ` in the SAMPLES object,
// however much (or little) it currently contains. Idempotent: re-running just
// re-fills the same slot. Matches from the opening backtick to the closing
// backtick, allowing escaped backticks (\`) inside the body.
function fillSample(content, flavor, code) {
    const escaped = escapeForTemplateLiteral(code);
    const slot = new RegExp('(\\b' + flavor + ':\\s*`)(?:\\\\.|[^`\\\\])*(`)');
    if (!slot.test(content)) {
        throw new Error('Could not find SAMPLES.' + flavor + ' template-literal slot in playground');
    }
    return content.replace(slot, (_m, open, close) => open + escaped + close);
}

function updatePlayground() {
    try {
        if (!fs.existsSync(PLAYGROUND_PATH)) {
            console.error('❌ Playground file not found:', PLAYGROUND_PATH);
            return false;
        }
        let content = fs.readFileSync(PLAYGROUND_PATH, 'utf8');

        for (const [flavor, srcPath] of Object.entries(SHOWCASES)) {
            if (!fs.existsSync(srcPath)) {
                console.error('❌ Showcase file not found:', srcPath);
                return false;
            }
            const code = fs.readFileSync(srcPath, 'utf8');
            content = fillSample(content, flavor, code);
            console.log('📖 Filled SAMPLES.' + flavor + ' (' + code.length + ' chars)');
        }

        fs.writeFileSync(PLAYGROUND_PATH, content, 'utf8');
        console.log('✅ Playground seeded with the tested showcase in both flavors!');
        return true;
    } catch (error) {
        console.error('❌ Error updating playground:', error.message);
        return false;
    }
}

// Run the update
if (require.main === module) {
    console.log('🚀 Updating Osprey Playground with the tested showcase (both flavors)...');
    const success = updatePlayground();
    process.exit(success ? 0 : 1);
}

module.exports = { updatePlayground };
