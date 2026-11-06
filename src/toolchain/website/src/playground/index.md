---
layout: base.njk
title: "Osprey Playground"
description: "Try Osprey programming language online with interactive code examples and real-time compilation"
date: "git Last Modified"
---

<link rel="stylesheet" data-name="vs/editor/editor.main" href="https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.45.0/min/vs/editor/editor.main.min.css">

<style>
    /* Override website layout constraints for playground area */
    .main-content {
        padding: 0 !important;
        margin: 0 !important;
        max-width: none !important;
    }
    
    .playground-container {
        display: flex;
        flex-direction: column;
        background: #1e1e1e;
        color: #d4d4d4;
        font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
        min-height: calc(100vh - 80px);
        height: calc(100vh - 80px);
    }
    
    .main {
        display: flex;
        flex: 1;
        overflow: hidden;
        min-height: 0;
    }
    
    .editor-container {
        flex: 1;
        display: flex;
        flex-direction: column;
        min-height: 0;
    }
    
    .editor-header {
        background: #2d2d30;
        padding: 10px 20px;
        display: flex;
        justify-content: space-between;
        align-items: center;
        border-bottom: 1px solid #444;
        flex-shrink: 0;
    }
    
    .editor-title {
        display: flex;
        align-items: center;
        gap: 10px;
        font-size: 14px;
    }
    
    .playground-badge {
        font-size: 12px;
        color: #569cd6;
        opacity: 0.8;
    }

    .flavor-toggle {
        display: inline-flex;
        margin-left: 8px;
        border: 1px solid #444;
        border-radius: 6px;
        overflow: hidden;
    }

    .flavor-btn {
        background: transparent;
        color: #9aa0a6;
        border: none;
        margin: 0;
        padding: 4px 12px;
        font-size: 12px;
        font-family: 'Consolas', 'Monaco', monospace;
        border-radius: 0;
        cursor: pointer;
    }

    .flavor-btn:hover { background: #3a3a3a; color: #d4d4d4; }

    .flavor-btn.active {
        background: #0e639c;
        color: #fff;
    }
    
    .header-right {
        display: flex;
        align-items: center;
        gap: 15px;
    }
    
    .status {
        display: flex;
        align-items: center;
        gap: 8px;
        font-size: 12px;
    }
    
    .status-dot {
        width: 8px;
        height: 8px;
        border-radius: 50%;
        background: #ffa500;
    }
    
    .status-dot.connected {
        background: #5a8a6b;
    }
    
    .status-dot.error {
        background: #f44747;
    }
    
    .button-group {
        display: flex;
        gap: 0;
    }
    
    #editor {
        flex: 1;
        min-height: 0;
        height: 100%;
    }
    
    .output-container {
        width: 400px;
        display: flex;
        flex-direction: column;
        border-left: 1px solid #444;
        min-height: 0;
    }
    
    .output-header {
        background: #2d2d30;
        padding: 10px 20px;
        border-bottom: 1px solid #444;
        display: flex;
        justify-content: space-between;
        align-items: center;
        flex-shrink: 0;
    }
    
    #output {
        flex: 1;
        padding: 20px;
        overflow-y: auto;
        font-family: 'Consolas', 'Monaco', monospace;
        white-space: pre-wrap;
        min-height: 0;
        background: #1e1e1e;
        color: #d4d4d4;
        line-height: 1.4;
    }
    
    #output.error {
        color: #d4d4d4;
        background: #1e1e1e;
        border-left: none;
    }
    
    #output.success {
        color: #d4d4d4;
        background: #1e1e1e;
        border-left: none;
    }
    
    #output.warning {
        color: #ffa500;
        background: #2d2d1b;
        border-left: 3px solid #ffa500;
    }
    
    .output-section {
        margin-bottom: 20px;
    }
    
    .output-section:last-child {
        margin-bottom: 0;
    }
    
    .output-label {
        font-size: 12px;
        text-transform: uppercase;
        opacity: 0.7;
        margin-bottom: 8px;
        font-weight: 600;
        letter-spacing: 0.5px;
    }
    
    .compiler-output {
        color: #d4d4d4;
        background: transparent;
        padding: 0;
        border: none;
        margin-bottom: 12px;
    }
    
    .program-output {
        color: #7cb992;
        background: rgba(124, 185, 146, 0.08);
        padding: 12px;
        border-radius: 4px;
        border-left: 3px solid #5a8a6b;
    }
    
    .program-output.empty {
        display: none;
    }
    
    .line-number {
        color: #569cd6;
        font-weight: bold;
    }
    
    /* Error listview styles */
    .error-list {
        display: grid;
        gap: 1px;
        font-family: 'Consolas', 'Monaco', monospace;
        font-size: 13px;
        line-height: 1.4;
    }
    
    .error-line {
        display: grid;
        grid-template-columns: auto 1fr;
        gap: 12px;
        padding: 8px 12px;
        background: #2d2d30;
        border: 1px solid #444;
        cursor: pointer;
        transition: all 0.2s ease;
        align-items: center;
    }
    
    .error-line:hover {
        background: #3c3c3c;
        border-color: #569cd6;
    }
    
    .error-line.selected {
        background: #404040;
        border-color: #569cd6;
        box-shadow: 0 0 0 1px #569cd6;
    }
    
    .error-location {
        color: #569cd6;
        font-weight: bold;
        font-size: 12px;
        white-space: nowrap;
        cursor: pointer;
        text-decoration: none;
    }
    
    .error-location:hover {
        text-decoration: underline;
    }
    
    .error-message {
        color: #f44747;
        flex: 1;
        word-break: break-word;
    }
    
    /* Editor error highlighting */
    .highlighted-error-line {
        background: rgba(244, 71, 71, 0.15) !important;
        border-left: 2px solid #f44747 !important;
    }
    
    .error-glyph {
        background: #f44747;
        width: 4px !important;
    }
    
    /* Splitter styles */
    .splitter {
        background: #444;
        cursor: col-resize;
        position: relative;
        flex-shrink: 0;
        width: 4px;
        transition: background-color 0.2s ease;
    }
    
    .splitter:hover {
        background: #569cd6;
    }
    
    .splitter::before {
        content: '';
        position: absolute;
        top: 50%;
        left: 50%;
        transform: translate(-50%, -50%);
        width: 2px;
        height: 20px;
        background: #666;
        border-radius: 1px;
    }
    
    .splitter.dragging {
        background: #569cd6;
    }
    
    /* Mobile responsiveness */
    @media (max-width: 768px) {
        .playground-container {
            height: 100vh;
            min-height: 100vh;
        }
        
        .main {
            flex-direction: column;
        }
        
        .editor-container {
            flex: 1;
        }
        
        .output-container {
            width: 100%;
            height: 40%;
            border-left: none;
            border-top: 1px solid #444;
        }
        
        .splitter {
            cursor: row-resize;
            width: 100%;
            height: 4px;
            border-top: none;
        }
        
        .splitter::before {
            width: 20px;
            height: 2px;
        }
        
        .editor-header {
            padding: 8px 15px;
            flex-wrap: wrap;
            gap: 8px;
        }
        
        .header-right {
            gap: 10px;
        }
        
        .editor-title {
            gap: 5px;
            font-size: 13px;
            flex-wrap: wrap;
            width: 100%;
        }

        .flavor-toggle { margin-left: 0; }
        
        .playground-badge {
            display: none;
        }
        
        .status {
            gap: 5px;
            font-size: 11px;
        }
        
        button {
            padding: 6px 12px;
            font-size: 13px;
            margin-left: 5px;
        }
        
        .output-header {
            padding: 8px 15px;
        }
        
        #output {
            padding: 15px;
        }
    }
    
    @media (max-width: 480px) {
        .editor-header, .output-header {
            padding: 6px 10px;
        }
        
        .header-right {
            gap: 8px;
        }
        
        .editor-title {
            font-size: 12px;
        }
        
        .status {
            font-size: 10px;
        }
        
        button {
            padding: 5px 8px;
            font-size: 12px;
            margin-left: 3px;
        }
        
        #output {
            padding: 10px;
            font-size: 13px;
        }
        

    }
    
    button {
        background: #0e639c;
        color: white;
        border: none;
        padding: 8px 16px;
        border-radius: 4px;
        cursor: pointer;
        font-size: 14px;
        margin-left: 10px;
    }
    
    button:hover {
        background: #1177bb;
    }
    
    button.primary {
        background: #16825d;
    }
    
    button.primary:hover {
        background: #1ea571;
    }

    .playground-outage {
        margin: 0;
        padding: 10px 16px;
        background: #3a2f12;
        color: #f5d78e;
        border-bottom: 1px solid #6b5520;
        font-size: 14px;
    }
    .playground-outage a { color: #ffe3a3; }
</style>

<div class="playground-container">
    <h1 class="sr-only">Osprey Playground</h1>
    <p id="playground-outage" class="playground-outage" role="status">The playground is offline while its servers wait for the Ubuntu fix for <a href="https://ubuntu.com/security/CVE-2026-98432">CVE-2026-98432</a>, a Linux kernel TCP flaw. It will be back soon; you can still read and edit the samples.</p>
    <div class="main">
        <div class="editor-container">
            <div class="editor-header">
                <div class="editor-title">
                    <span>Osprey Editor</span>
                    <span class="playground-badge">⚡ Playground</span>
                    <div class="flavor-toggle" role="group" aria-label="Syntax flavor">
                        <button id="flavor-osp" class="flavor-btn active" onclick="setFlavor('osp')" aria-pressed="true">Default .osp</button>
                        <button id="flavor-ospml" class="flavor-btn" onclick="setFlavor('ospml')" aria-pressed="false">ML .ospml</button>
                        <button id="flavor-py" class="flavor-btn" onclick="setFlavor('py')" aria-pressed="false">Python .py</button>
                    </div>
                </div>
                <div class="header-right">
                    <div class="status">
                        <div id="status-dot" class="status-dot"></div>
                        <span id="status-text">Connecting...</span>
                    </div>
                    <div class="button-group">
                        <button onclick="compileCode()">Compile</button>
                        <button class="primary" onclick="runCode()">Run</button>
                    </div>
                </div>
            </div>
            <div id="editor"></div>
        </div>
        
        <div id="splitter" class="splitter"></div>
        
        <div class="output-container">
            <div class="output-header">
                <span>Output</span>
                <button onclick="clearOutput()">Clear</button>
            </div>
            <div id="output"></div>
        </div>
    </div>
</div>

<!-- Load Monaco from CDN -->
<script src="https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.45.0/min/vs/loader.min.js"></script>

<script>
    let editor;
    const API_URL = 'https://osprey.fly.dev/api';
    // An unreachable compiler fails the request instead of leaving it running.
    const COMPILER_TIMEOUT_MS = 30000;
    
    // Initialize Monaco Editor
    require.config({ paths: { vs: 'https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.45.0/min/vs' } });
    
    require(['vs/editor/editor.main'], function() {
        // Register Osprey language (shared tokenizer for both flavors)
        monaco.languages.register({ id: 'osprey' });

        // Monarch grammar covering BOTH flavors: braces/`fn`/named args (.osp)
        // and offside-rule/whitespace-application (.ospml). Handles effects,
        // fibers, `${...}` string interpolation, types, numbers and operators.
        monaco.languages.setMonarchTokensProvider('osprey', {
            keywords: [
                'fn', 'let', 'mut', 'type', 'import', 'module', 'match', 'if', 'else',
                'loop', 'spawn', 'await', 'yield', 'extern', 'effect', 'perform',
                'handle', 'resume', 'in', 'do', 'true', 'false'
            ],
            typeKeywords: ['int', 'string', 'bool', 'Unit', 'float', 'char'],
            operators: [
                '=>', '->', '|>', ':=', '==', '!=', '<=', '>=', '&&', '||',
                '=', '+', '-', '*', '/', '%', '<', '>', '!', ':', '|', '\\'
            ],
            symbols: /[=><!~?:&|+\-*\/^%\\]+/,
            tokenizer: {
                root: [
                    // ML-flavor (* … *) block comments (incl. (** *) docs); nest via @push.
                    [/\(\*/, 'comment', '@blockComment'],
                    [/\/\/.*$/, 'comment'],
                    // Type / union-variant names start with a capital letter.
                    [/[A-Z][\w$]*/, 'type'],
                    [/[a-z_$][\w$]*/, {
                        cases: {
                            '@keywords': 'keyword',
                            '@typeKeywords': 'type',
                            '@default': 'identifier'
                        }
                    }],
                    { include: '@whitespace' },
                    [/"/, { token: 'string.quote', bracket: '@open', next: '@string' }],
                    [/\d+/, 'number'],
                    [/@symbols/, { cases: { '@operators': 'operator', '@default': '' } }],
                ],
                whitespace: [
                    [/[ \t\r\n]+/, ''],
                ],
                // Nesting (* … *) block comment: everything stays 'comment'.
                blockComment: [
                    [/\(\*/, 'comment', '@push'],
                    [/\*\)/, 'comment', '@pop'],
                    [/[^(*]+/, 'comment'],
                    [/[(*]/, 'comment'],
                ],
                // String literals with `${...}` interpolation highlighted as code.
                string: [
                    [/\$\{/, { token: 'delimiter.bracket', next: '@interp' }],
                    [/[^"\\$]+/, 'string'],
                    [/\\./, 'string.escape'],
                    [/\$/, 'string'],
                    [/"/, { token: 'string.quote', bracket: '@close', next: '@pop' }],
                ],
                interp: [
                    [/\}/, { token: 'delimiter.bracket', next: '@pop' }],
                    [/[A-Z][\w$]*/, 'type'],
                    [/[a-z_$][\w$]*/, { cases: { '@keywords': 'keyword', '@default': 'identifier' } }],
                    [/\d+/, 'number'],
                    [/@symbols/, { cases: { '@operators': 'operator', '@default': '' } }],
                ],
            }
        });

        // Create editor (starts in the Default flavor).
        editor = monaco.editor.create(document.getElementById('editor'), {
            value: SAMPLES.osp,
            language: 'osprey',
            theme: 'vs-dark',
            automaticLayout: true
        });

        // Update status
        updateStatus('connected', 'Ready');
    });

    // One tested program, the package manager's install, in all three flavors.
    // Switch the editor contents with the flavor toggle in the header.
    let currentFlavor = 'osp';
    const SAMPLES = {
        // @generated:osp — filled from src/osprey/tests/effects/install/install.test.osp by scripts/update-playground.js
        osp: `/// The package manager's file-system effect: one install, a real disk and
/// three test doubles. The Playground opens on this program in every flavor.
effect Files {
    read: fn(string) -> Result<string, Error>
    write: fn(string, string) -> Result<int, Error>
}

/// Write the lock file; if that fails, put the old manifest back.
fn commitLock(previous, lock) = match perform Files.write("osprey.lock", lock) {
    Success _ => "installed"
    Error { message } => match perform Files.write("osprey.toml", previous) {
        Success _ => "rolled back: \${message}"
        Error _ => "manifest left changed: \${message}"
    }
}

/// Update the manifest, then commit the lock. It never names a disk.
fn install(manifest, lock) = match perform Files.read("osprey.toml") {
    Error { message } => "no manifest: \${message}"
    Success { value } => match perform Files.write("osprey.toml", manifest) {
        Error { message } => "manifest not written: \${message}"
        Success _ => commitLock(value, lock)
    }
}

// Production passes the real disk (the Playground sandbox has none):
// handler Files { read path => readFile(path)  write path text => writeFile(path, text) }

/// Test doubles: canned disks, no files touched.
let healthyDisk = handler Files {
    read _ => Success { value: "version = 1" }
    write _ text => Success { value: byteLength(text) }
}

let fullDisk = handler Files {
    read _ => Success { value: "version = 1" }
    write path text => if path == "osprey.lock" { Error { message: "disk full" } } else { Success { value: byteLength(text) } }
}

let readOnlyDisk = handler Files {
    read _ => Success { value: "version = 1" }
    write _ _ => Error { message: "read-only" }
}

fn upgrade() = install("version = 2", "locked")

test("a healthy disk installs", fn() => expect(healthyDisk(upgrade), "installed"))
test("a full disk rolls the manifest back", fn() => expect(fullDisk(upgrade), "rolled back: disk full"))
test("a read-only disk changes nothing", fn() => expect(readOnlyDisk(upgrade), "manifest not written: read-only"))
`,
        // @generated:ospml — filled from src/osprey/tests/effects/install/install.test.ospml by scripts/update-playground.js
        ospml: `// The package manager's file-system effect: one install, a real disk and
// three test doubles. The Playground opens on this program in every flavor.
effect Files
    read : string => Result<string, Error>
    write : (string, string) => Result<int, Error>

// Write the lock file; if that fails, put the old manifest back.
commitLock (previous, lock) = match perform Files.write "osprey.lock" lock
    Success _ => "installed"
    Error message => match perform Files.write "osprey.toml" previous
        Success _ => "rolled back: \${message}"
        Error _ => "manifest left changed: \${message}"

// Update the manifest, then commit the lock. It never names a disk.
install (manifest, lock) = match perform Files.read "osprey.toml"
    Error message => "no manifest: \${message}"
    Success value => match perform Files.write "osprey.toml" manifest
        Error message => "manifest not written: \${message}"
        Success _ => commitLock (value, lock)

// Production passes the real disk (the Playground sandbox has none):
// handler Files { read path => readFile path  write path text => writeFile path text }

// Test doubles: canned disks, no files touched.
healthyDisk = handler Files
    read _ => Success(value = "version = 1")
    write _ text => Success(value = byteLength text)

fullDisk = handler Files
    read _ => Success(value = "version = 1")
    write path text => match path == "osprey.lock"
        true => Error(message = "disk full")
        false => Success(value = byteLength text)

readOnlyDisk = handler Files
    read _ => Success(value = "version = 1")
    write _ _ => Error(message = "read-only")

upgrade () = install ("version = 2", "locked")

test "a healthy disk installs" (\\() => expect (healthyDisk upgrade) "installed")
test "a full disk rolls the manifest back" (\\() => expect (fullDisk upgrade) "rolled back: disk full")
test "a read-only disk changes nothing" (\\() => expect (readOnlyDisk upgrade) "manifest not written: read-only")
`,
        // @generated:py — filled from src/osprey/tests/effects/install/install.test.py by scripts/update-playground.js
        py: `# osprey: flavor=python

# The package manager's file-system effect: one install, a real disk and
# three test doubles. The Playground opens on this program in every flavor.
@effect
class Files:
    def read(path: str) -> Result[str, Error]: ...
    def write(path: str, text: str) -> Result[int, Error]: ...


def commit_lock(previous, lock):
    match perform(Files.write("osprey.lock", lock)):
        case Success():
            return "installed"
        case Error(message=message):
            match perform(Files.write("osprey.toml", previous)):
                case Success():
                    return f"rolled back: {message}"
                case Error():
                    return f"manifest left changed: {message}"


def install(manifest, lock):
    match perform(Files.read("osprey.toml")):
        case Error(message=message):
            return f"no manifest: {message}"
        case Success(value=previous):
            match perform(Files.write("osprey.toml", manifest)):
                case Error(message=message):
                    return f"manifest not written: {message}"
                case Success():
                    return commit_lock(previous, lock)


# Production passes the real disk (the Playground sandbox has none):
# @handler(Files) class disk: read returns readFile(path), write returns writeFile(path, text)


# Test doubles: canned disks, no files touched.
@handler(Files)
class healthy_disk:
    def read(_path):
        return Success(value="version = 1")

    def write(_path, text):
        return Success(value=byteLength(text))


@handler(Files)
class full_disk:
    def read(_path):
        return Success(value="version = 1")

    def write(path, text):
        return Error(message="disk full") if path == "osprey.lock" else Success(value=byteLength(text))


@handler(Files)
class read_only_disk:
    def read(_path):
        return Success(value="version = 1")

    def write(_path, _text):
        return Error(message="read-only")


def upgrade():
    return install("version = 2", "locked")


def main():
    test("a healthy disk installs", lambda: expect(healthy_disk(upgrade), "installed"))
    test("a full disk rolls the manifest back", lambda: expect(full_disk(upgrade), "rolled back: disk full"))
    with read_only_disk:
        test("a read-only disk changes nothing", lambda: expect(upgrade(), "manifest not written: read-only"))
`,
    };

    const API_FLAVORS = { osp: 'default', ospml: 'ml', py: 'python' };

    // Select a syntax flavor and its sample.
    // Only swaps when the buffer is still an unedited sample, so we never clobber
    // a user's work; otherwise it just flips the active button label.
    function setFlavor(flavor) {
        if (flavor === currentFlavor) return;
        for (const name of Object.keys(SAMPLES)) {
            const button = document.getElementById(`flavor-${name}`);
            button.classList.toggle('active', name === flavor);
            button.setAttribute('aria-pressed', String(name === flavor));
        }

        if (editor) {
            const current = editor.getValue();
            const isPristine = Object.values(SAMPLES).includes(current);
            if (isPristine) editor.setValue(SAMPLES[flavor]);
            monaco.editor.setModelLanguage(editor.getModel(), flavor === 'py' ? 'python' : 'osprey');
        }
        currentFlavor = flavor;
    }
    
    function updateStatus(type, message) {
        const statusDot = document.getElementById('status-dot');
        const statusText = document.getElementById('status-text');
        
        statusDot.className = `status-dot ${type}`;
        statusText.textContent = message;
    }

    async function compileCode() {
        const code = editor.getValue();
        const output = document.getElementById('output');
        
        updateStatus('', 'Compiling...');
        output.innerHTML = '<div style="color: #ffa500;">Compiling...</div>';
        
        try {
            const response = await fetch(`${API_URL}/compile`, {
                method: 'POST',
                signal: AbortSignal.timeout(COMPILER_TIMEOUT_MS),
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ code, flavor: API_FLAVORS[currentFlavor] })
            });
            
            let result;
            const contentType = response.headers.get('content-type');
            
            if (contentType && contentType.includes('application/json')) {
                result = await response.json();
            } else {
                // Handle non-JSON responses (like 500 errors)
                const text = await response.text();
                result = { success: false, error: text || `HTTP ${response.status}: ${response.statusText}` };
            }
            
            if (!response.ok) {
                // Handle HTTP errors (400, 500, etc.)
                output.className = 'error';
                let errorMessage = result.error || `HTTP ${response.status}: ${response.statusText}`;
                
                if (response.status === 500) {
                    errorMessage = 'Internal server error occurred. Please try again or contact support if the issue persists.';
                } else if (response.status === 502) {
                    errorMessage = result.error || 'The compiler encountered an internal error. Please report this code to help us fix the issue.';
                }
                
                output.innerHTML = formatErrorOutput(errorMessage);
                updateStatus('error', 'Compilation failed');
                return;
            }
            
            if (result.success) {
                // Successful compilation
                output.className = 'success';
                let outputText = '';
                
                if (result.programOutput && result.programOutput.trim()) {
                    outputText = formatPlainOutput(result.programOutput);
                } else {
                    outputText = '✅ Compilation successful - no output';
                }
                
                output.innerHTML = outputText;
                updateStatus('connected', 'Ready');
            } else {
                // Compilation failed
                output.className = 'error';
                output.innerHTML = formatErrorOutput(result.error || 'Unknown compilation error');
                updateStatus('error', 'Compilation failed');
            }
            
        } catch (error) {
            output.className = 'error';
            output.innerHTML = formatErrorOutput(`Failed to connect to compiler: ${error.message}\n\n${document.getElementById('playground-outage').textContent}`);
            updateStatus('error', 'Offline');
        }
    }
    
    async function runCode() {
        const code = editor.getValue();
        const output = document.getElementById('output');
        
        updateStatus('', 'Running...');
        output.innerHTML = '<div style="color: #ffa500;">Compiling and running...</div>';
        
        try {
            const response = await fetch(`${API_URL}/run`, {
                method: 'POST',
                signal: AbortSignal.timeout(COMPILER_TIMEOUT_MS),
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ code, flavor: API_FLAVORS[currentFlavor] })
            });
            
            let result;
            const contentType = response.headers.get('content-type');
            
            if (contentType && contentType.includes('application/json')) {
                result = await response.json();
            } else {
                // Handle non-JSON responses (like 500 errors)
                const text = await response.text();
                result = { success: false, error: text || `HTTP ${response.status}: ${response.statusText}` };
            }
            
            if (!response.ok) {
                // Handle HTTP errors (400, 500, etc.)
                output.className = 'error';
                let errorMessage = result.error || `HTTP ${response.status}: ${response.statusText}`;
                
                if (response.status === 500) {
                    errorMessage = 'Internal server error occurred. Please try again or contact support if the issue persists.';
                } else if (response.status === 502) {
                    errorMessage = result.error || 'The compiler encountered an internal error. Please report this code to help us fix the issue.';
                }
                
                const statusMessage = result.isCompilationError ? 'Compilation failed' : 'Execution failed';
                output.innerHTML = formatErrorOutput(errorMessage);
                updateStatus('error', statusMessage);
                return;
            }
            
            if (result.success) {
                // Successful execution
                output.className = 'success';
                let outputText = '';
                
                if (result.programOutput && result.programOutput.trim()) {
                    outputText = result.programOutput;
                } else {
                    outputText = '✅ Program ran successfully - no output';
                }
                
                output.innerHTML = formatPlainOutput(outputText);
                updateStatus('connected', 'Ready');
            } else {
                // Execution failed
                output.className = 'error';
                output.innerHTML = formatErrorOutput(result.error || 'Unknown error');
                updateStatus('error', 'Execution failed');
            }
            
        } catch (error) {
            output.className = 'error';
            output.innerHTML = formatErrorOutput(`Failed to connect to compiler: ${error.message}\n\n${document.getElementById('playground-outage').textContent}`);
            updateStatus('error', 'Offline');
        }
    }
    
    function formatErrorOutput(text) {
        if (!text) return '';
        
        // Escape HTML
        text = text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
        
        // Split by lines and parse errors
        const lines = text.split('\n').filter(line => line.trim());
        const errorLines = [];
        
        lines.forEach(line => {
            // Check if line contains line number references
            const lineNumberMatch = line.match(/\b(?:line\s+)(\d+)(?:\s*:\s*(\d+))?/i) ||
                                  line.match(/\bat line\s+(\d+)/i) ||
                                  line.match(/\berror at\s+(\d+)/i) ||
                                  line.match(/\[(\d+)(?:\s*:\s*(\d+))?\]/);
            
            if (lineNumberMatch) {
                const lineNum = parseInt(lineNumberMatch[1]);
                const column = lineNumberMatch[2] ? parseInt(lineNumberMatch[2]) : 0;
                
                // Extract the error message (everything after the line number)
                let message = line.replace(/^.*?(?:line\s+\d+(?::\d+)?|at line\s+\d+|\[\d+(?::\d+)?\])\s*/, '').trim();
                if (!message) message = line.trim();
                
                errorLines.push({
                    lineNum,
                    column,
                    message,
                    fullText: line
                });
            } else {
                // Non-line-specific error
                errorLines.push({
                    lineNum: null,
                    column: null,
                    message: line.trim(),
                    fullText: line
                });
            }
        });
        
        if (errorLines.length === 0) {
            return text; // Fallback to original text
        }
        
        // Build clean grid structure
        const gridItems = errorLines.map(error => {
            if (error.lineNum !== null) {
                const location = error.column > 0 ? `${error.lineNum}:${error.column}` : `${error.lineNum}`;
                return `<div class="error-line" onclick="jumpToLine(event, ${error.lineNum}, ${error.column || 1})">
                    <span class="error-location">Line ${location}</span>
                    <span class="error-message">${error.message}</span>
                </div>`;
            } else {
                return `<div class="error-line">
                    <span class="error-location">—</span>
                    <span class="error-message">${error.message}</span>
                </div>`;
            }
        });
        
        return `<div class="error-list">${gridItems.join('')}</div>`;
    }
    
    function formatPlainOutput(text) {
        if (!text) return '';
        
        // Escape HTML
        text = text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
        
        // Color specific messages
        text = text.replace(/(Program executed successfully)/g, '<span style="color: #7cb992;">$1</span>');
        text = text.replace(/(Running program\.\.\.)/g, '<span style="color: #ffa500;">$1</span>');
        
        return text;
    }
    
    function jumpToLine(evt, lineNumber, column = 1) {
        if (!editor) return;
        
        console.log(`🎯 Jumping to line ${lineNumber}, column ${column}`);
        
        // Remove any existing selections
        const errorLines = document.querySelectorAll('.error-line');
        errorLines.forEach(el => el.classList.remove('selected'));
        
        // Mark clicked line as selected (event passed explicitly — no reliance
        // on the non-standard implicit global `event`)
        evt?.target?.closest('.error-line')?.classList.add('selected');
        
        // Jump to the line in Monaco editor
        editor.setPosition({ lineNumber: lineNumber, column: column });
        editor.revealLineInCenter(lineNumber);
        editor.focus();
        
        // Optionally highlight the line temporarily
        const decoration = editor.deltaDecorations([], [{
            range: new monaco.Range(lineNumber, 1, lineNumber, 1),
            options: {
                isWholeLine: true,
                className: 'highlighted-error-line',
                glyphMarginClassName: 'error-glyph'
            }
        }]);
        
        // Remove decoration after 2 seconds
        setTimeout(() => {
            editor.deltaDecorations(decoration, []);
        }, 2000);
    }
    
    function clearOutput() {
        document.getElementById('output').innerHTML = '';
        document.getElementById('output').className = '';
    }
    
    // Splitter functionality
    let isDragging = false;
    let startX = 0;
    let startY = 0;
    let startWidth = 0;
    let startHeight = 0;
    let isMobile = false;
    
    function initSplitter() {
        const splitter = document.getElementById('splitter');
        const editorContainer = document.querySelector('.editor-container');
        const outputContainer = document.querySelector('.output-container');
        
        if (!splitter || !editorContainer || !outputContainer) return;
        
        splitter.addEventListener('mousedown', startDrag);
        document.addEventListener('mousemove', drag);
        document.addEventListener('mouseup', stopDrag);
        
        // Touch events for mobile
        splitter.addEventListener('touchstart', startDrag);
        document.addEventListener('touchmove', drag);
        document.addEventListener('touchend', stopDrag);
        
        // Check if mobile layout
        function checkMobile() {
            isMobile = window.innerWidth <= 768;
        }
        
        checkMobile();
        window.addEventListener('resize', checkMobile);
    }
    
    function startDrag(e) {
        isDragging = true;
        const splitter = document.getElementById('splitter');
        const editorContainer = document.querySelector('.editor-container');
        const outputContainer = document.querySelector('.output-container');
        
        splitter.classList.add('dragging');
        
        if (isMobile) {
            startY = e.touches ? e.touches[0].clientY : e.clientY;
            startHeight = editorContainer.offsetHeight;
        } else {
            startX = e.touches ? e.touches[0].clientX : e.clientX;
            startWidth = editorContainer.offsetWidth;
        }
        
        e.preventDefault();
    }
    
    function drag(e) {
        if (!isDragging) return;
        
        const main = document.querySelector('.main');
        const editorContainer = document.querySelector('.editor-container');
        const outputContainer = document.querySelector('.output-container');
        
                 if (isMobile) {
             const currentY = e.touches ? e.touches[0].clientY : e.clientY;
             const deltaY = currentY - startY;
             const newHeight = startHeight + deltaY;
             const mainHeight = main.offsetHeight;
             
             if (newHeight >= 0 && newHeight <= mainHeight) {
                 const heightPercent = (newHeight / mainHeight) * 100;
                 const outputPercent = 100 - heightPercent;
                 
                 editorContainer.style.flex = 'none';
                 editorContainer.style.height = `${heightPercent}%`;
                 outputContainer.style.height = `${outputPercent}%`;
             }
         } else {
             const currentX = e.touches ? e.touches[0].clientX : e.clientX;
             const deltaX = currentX - startX;
             const newWidth = startWidth + deltaX;
             const mainWidth = main.offsetWidth;
             
             if (newWidth >= 0 && newWidth <= mainWidth) {
                 const widthPercent = (newWidth / mainWidth) * 100;
                 const outputWidth = mainWidth - newWidth - 4; // Account for splitter width
                 
                 editorContainer.style.flex = 'none';
                 editorContainer.style.width = `${newWidth}px`;
                 outputContainer.style.width = `${outputWidth}px`;
             }
         }
        
        e.preventDefault();
    }
    
    function stopDrag() {
        if (!isDragging) return;
        
        isDragging = false;
        const splitter = document.getElementById('splitter');
        splitter.classList.remove('dragging');
    }
    
    // Initialize splitter when page loads
    window.addEventListener('load', initSplitter);
</script> 
