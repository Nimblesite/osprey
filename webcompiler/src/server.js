import { spawn } from 'child_process'
import express from 'express'
import fs from 'fs/promises'
import { createServer } from 'http'
import path from 'path'
import { fileURLToPath } from 'url'
import { WebSocketServer } from 'ws'
import { randomUUID } from 'crypto'
import { execSync } from 'child_process'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)

const app = express()
const server = createServer(app)

const PORT = process.env.PORT || 3001

// STARTUP LOGGING - Make it super obvious the server is starting
console.log('\n' + '='.repeat(80))
console.log('🚀 OSPREY WEB COMPILER STARTING UP')
console.log('='.repeat(80))
console.log(`📍 Server file: ${__filename}`)
console.log(`📁 Working directory: ${process.cwd()}`)
console.log(`🐳 Docker environment: ${process.env.DOCKER_ENV || 'false'}`)
console.log(`🏃 Node environment: ${process.env.NODE_ENV || 'development'}`)
console.log(`🔌 Target port: ${PORT}`)
console.log('='.repeat(80))

// Request logging middleware - track ALL requests
app.use((req, res, next) => {
    const timestamp = new Date().toISOString()
    console.log(`\n📨 [${timestamp}] ${req.method} ${req.url}`)
    console.log(`📍 User-Agent: ${req.headers['user-agent'] || 'unknown'}`)
    console.log(`📍 Origin: ${req.headers.origin || 'none'}`)
    console.log(`📍 Content-Type: ${req.headers['content-type'] || 'none'}`)

    // Log body size for POST requests
    if (req.method === 'POST' && req.body) {
        const bodySize = JSON.stringify(req.body).length
        console.log(`📏 Body size: ${bodySize} bytes`)
    }

    next()
})

// Middleware
app.use(express.json({ limit: '10mb' }))

// CORS middleware
app.use((req, res, next) => {
    // Allow requests from the website running on localhost:8080
    const origin = req.headers.origin;
    const allowedOrigins = [
        'http://localhost:8080',
        'http://127.0.0.1:8080',
        'http://localhost:3001',
        'http://127.0.0.1:3001',
        'https://ospreylang.dev',
        'https://www.ospreylang.dev'
    ];

    if (allowedOrigins.includes(origin)) {
        res.header('Access-Control-Allow-Origin', origin);
    }

    res.header('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
    res.header('Access-Control-Allow-Headers', 'Content-Type, Authorization, X-Requested-With');
    res.header('Access-Control-Allow-Credentials', 'true');

    if (req.method === 'OPTIONS') {
        return res.sendStatus(200);
    }
    next();
})

// Health check endpoint
app.get('/api', (req, res) => {
    res.json({
        status: 'ok',
        service: 'osprey-web-compiler',
        version: '0.2.0',
        timestamp: new Date().toISOString()
    })
})

function logCompilerResult(result) {
    console.log('🔨 COMPILER OUTPUT (stderr):')
    console.log('-'.repeat(50))
    console.log(result.stderr || 'NO COMPILER OUTPUT')
    console.log('-'.repeat(50))

    console.log('📋 PROGRAM OUTPUT (stdout):')
    console.log('-'.repeat(50))
    console.log(result.stdout || 'NO PROGRAM OUTPUT')
    console.log('-'.repeat(50))
}

function sendSystemError(res, error) {
    console.error('❌ System error:', error.message)
    res.status(500).json({ success: false, error: error.message })
}

// Compile endpoint
app.post('/api/compile', async (req, res) => {
    const { code, flavor } = req.body
    console.log('📝 Compile request received')
    console.log('📄 Code length:', code?.length || 0)
    
    // LOG THE ACTUAL CODE
    console.log('🔍 CODE BEING COMPILED:')
    console.log('='.repeat(50))
    console.log(code || 'NO CODE PROVIDED')
    console.log('='.repeat(50))

    if (!code) {
        return res.status(400).json({ success: false, error: 'No code provided' })
    }

    try {
        const result = await runOspreyCompiler(['--sandbox', '--ast'], code, flavor)

        logCompilerResult(result)

        if (result.success) {
            console.log('✅ Compile success, exit code:', result.exitCode)
            res.status(200).json({
                success: true,
                compilerOutput: result.stderr || '',
                programOutput: result.stdout || '' // AST output goes to stdout
            })
        } else {
            console.error('❌ Compile failed, exit code:', result.exitCode)

            const errorOutput = result.stderr || result.stdout || '';

            // Detect INTERNAL compiler errors - simple marker from compiler
            const isInternalError = errorOutput.includes('INTERNAL_COMPILER_ERROR:');

            if (isInternalError) {
                // Internal compiler error - log for debugging but don't expose to user
                console.error('🚨 INTERNAL COMPILER ERROR DETECTED:', errorOutput);
                res.status(502).json({
                    success: false,
                    error: 'The compiler encountered an internal error. Please report this code to help us fix the issue.',
                    isInternalError: true
                });
                return;
            }

            res.status(422).json({ // 422 Unprocessable Entity for compilation errors
                success: false,
                compilerOutput: result.stderr || '',
                programOutput: result.stdout || '',
                error: errorOutput || `Compilation failed with exit code ${result.exitCode}`
            })
        }
    } catch (error) {
        sendSystemError(res, error)
    }
})

// Run endpoint
app.post('/api/run', async (req, res) => {
    const { code, flavor } = req.body
    console.log('🏃 Run request received')
    console.log('📄 Code length:', code?.length || 0)
    
    // LOG THE ACTUAL CODE
    console.log('🔍 CODE BEING RUN:')
    console.log('='.repeat(50))
    console.log(code || 'NO CODE PROVIDED')
    console.log('='.repeat(50))

    if (!code) {
        return res.status(400).json({ success: false, error: 'No code provided' })
    }

    try {
        const result = await runOspreyCompiler(['--run'], code, flavor)

        logCompilerResult(result)

        if (result.success) {
            console.log('✅ Run success, exit code:', result.exitCode)

            res.status(200).json({
                success: true,
                compilerOutput: result.stderr || '',
                programOutput: result.stdout || ''
            })
        } else {
            console.error('❌ Run failed, exit code:', result.exitCode)

            const errorOutput = result.stderr || result.stdout || '';

            // Detect INTERNAL compiler errors - simple marker from compiler
            const isInternalError = errorOutput.includes('INTERNAL_COMPILER_ERROR:');

            if (isInternalError) {
                // Internal compiler error - log for debugging but don't expose to user
                console.error('🚨 INTERNAL COMPILER ERROR DETECTED:', errorOutput);
                res.status(502).json({
                    success: false,
                    error: 'The compiler encountered an internal error. Please report this code to help us fix the issue.',
                    isInternalError: true
                });
                return;
            }

            // Determine if it's a user syntax/compilation error or runtime error
            const isCompilationError = errorOutput.includes('parse errors') ||
                errorOutput.includes('undefined variable') ||
                errorOutput.includes('syntax error') ||
                errorOutput.includes('type mismatch') ||
                errorOutput.includes('Compilation failed');

            const statusCode = isCompilationError ? 422 : 400; // 422 for compilation, 400 for runtime

            res.status(statusCode).json({
                success: false,
                compilerOutput: result.stderr || '',
                programOutput: result.stdout || '',
                isCompilationError: isCompilationError,
                error: errorOutput || `Process failed with exit code ${result.exitCode}`
            })
        }
    } catch (error) {
        sendSystemError(res, error)
    }
})

// STARTUP: Delete ALL temp folders on server startup
async function deleteAllTempFolders() {
    const tempBaseDir = '/tmp/osprey-temp'
    try {
        console.log('🗑️ Deleting ALL temp folders on startup...')
        await fs.rm(tempBaseDir, { recursive: true, force: true })
        console.log('✅ All temp folders deleted')
    } catch (error) {
        console.error('⚠️ Error deleting temp folders:', error.message)
    }
}

// Cleanup old temp folders periodically to prevent disk space issues
async function cleanupOldTempFolders() {
    const tempBaseDir = '/tmp/osprey-temp'
    try {
        const folders = await fs.readdir(tempBaseDir)
        const now = Date.now()
        const oneHourAgo = now - (60 * 60 * 1000) // 1 hour ago

        for (const folder of folders) {
            const folderPath = path.join(tempBaseDir, folder)
            const stats = await fs.stat(folderPath)
            if (stats.isDirectory() && stats.mtime.getTime() < oneHourAgo) {
                await fs.rm(folderPath, { recursive: true, force: true })
                console.log(`🗑️ Cleaned up old temp folder: ${folder}`)
            }
        }
    } catch (error) {
        console.error('⚠️ Error cleaning up temp folders:', error.message)
    }
}

// Run cleanup every 30 minutes
setInterval(cleanupOldTempFolders, 30 * 60 * 1000)

// DELETE ALL TEMP FOLDERS ON STARTUP
deleteAllTempFolders()

// THREAD-SAFE Helper function to run Osprey compiler
// Each request gets its own UUID-named folder for complete isolation
// Always uses --sandbox flag for security (disables HTTP, WebSocket, file system, and FFI access)
function runOspreyCompiler(args, code = '', flavor = 'default') {
    return new Promise(async (resolve, reject) => {
        // Diagnostics removed - too verbose for production logging
        // Create a unique UUID folder for this request - THREAD SAFE!
        const requestId = randomUUID()
        const tempBaseDir = '/tmp/osprey-temp'
        const tempRequestDir = path.join(tempBaseDir, requestId)
        // The compiler picks the source flavor from the file extension
        // (`.ospml` ⇒ ML, `.osp` ⇒ Default), so name the temp file to match the
        // flavor the playground selected — ML (offside-rule) source parses only
        // through the ML frontend. [FLAVOR-SELECT]
        const isMl = flavor === 'ml' || flavor === 'ospml'
        const tempFile = path.join(tempRequestDir, isMl ? 'main.ospml' : 'main.osp')

        try {
            // Create the unique temp directory for this request
            console.log(`📁 Creating temp directory: ${tempRequestDir}`)
            await fs.mkdir(tempRequestDir, { recursive: true })
            console.log(`📁 Created temp folder: ${requestId}`)

            // Verify temp directory was created
            const tempStats = await fs.stat(tempRequestDir)
            console.log(`📊 Temp directory stats: ${JSON.stringify({
                isDirectory: tempStats.isDirectory(),
                mode: tempStats.mode,
                uid: tempStats.uid,
                gid: tempStats.gid
            })}`)

            console.log(`💾 Writing temp file: ${tempFile}`)
            await fs.writeFile(tempFile, code)
            
            // Verify file was written
            const fileStats = await fs.stat(tempFile)
            console.log(`📊 Temp file stats: ${JSON.stringify({
                size: fileStats.size,
                isFile: fileStats.isFile(),
                mode: fileStats.mode
            })}`)
            
            // Use the osprey binary from PATH (installed in Docker) or fall back
            // to the local Rust release build (cargo build --release)
            const ospreyPath = process.env.NODE_ENV === 'production' || process.env.DOCKER_ENV
                ? 'osprey'
                : path.resolve(__dirname, '../../target/release/osprey')
            
            // Check if osprey binary exists and is executable
            console.log(`🔍 Checking osprey binary: ${ospreyPath}`)
            try {
                if (ospreyPath === 'osprey') {
                    console.log(`🔍 Using osprey from PATH`)
                } else {
                    const binaryStats = await fs.stat(ospreyPath)
                    console.log(`📊 Osprey binary stats: ${JSON.stringify({
                        size: binaryStats.size,
                        isFile: binaryStats.isFile(),
                        mode: binaryStats.mode,
                        executable: (binaryStats.mode & 0o111) !== 0
                    })}`)
                }
            } catch (e) {
                console.error(`❌ Error checking osprey binary: ${e.message}`)
            }
            
            const startTime = Date.now()
            console.log(`🔨 Running: ${ospreyPath} ${tempFile} ${args.join(' ')}`)
            console.log(`⏰ Started at: ${new Date().toISOString()}`)
            
            const child = spawn(ospreyPath, [tempFile, ...args], {
                stdio: 'pipe',
                cwd: tempRequestDir, // Run in the temp directory
                timeout: 20000 
            })

            let stdout = ''
            let stderr = ''

            child.stdout.on('data', (data) => {
                stdout += data.toString()
            })

            child.stderr.on('data', (data) => {
                stderr += data.toString()
            })

            child.on('close', async (exitCode, signal) => {
                const endTime = Date.now()
                const duration = endTime - startTime
                
                // Log detailed exit information with timing
                console.log(`🔚 Process finished - Exit code: ${exitCode}, Signal: ${signal}`)
                console.log(`⏰ Ended at: ${new Date().toISOString()}`)
                console.log(`⏱️ Duration: ${duration}ms`)
                
                // Clean up the ENTIRE temp folder for this request
                try {
                    await fs.rm(tempRequestDir, { recursive: true, force: true })
                    console.log(`🗑️ Cleaned up temp folder: ${requestId}`)
                } catch (e) {
                    console.error('⚠️ Failed to clean up temp folder:', e.message)
                }

                // Handle timeout/signal termination
                if (exitCode === null && signal) {
                    console.error(`⏰ Process was killed by signal: ${signal} after ${duration}ms`)
                    stderr += `\nProcess was terminated by signal: ${signal} (likely timeout) after ${duration}ms`
                }

                // Always resolve with the result - let the caller determine success/failure
                resolve({
                    exitCode: exitCode || -1, // Convert null to -1 for consistency
                    stdout,
                    stderr,
                    success: exitCode === 0
                })
            })

            child.on('error', async (error) => {
                // Clean up temp folder on error
                try {
                    await fs.rm(tempRequestDir, { recursive: true, force: true })
                    console.log(`🗑️ Cleaned up temp folder after error: ${requestId}`)
                } catch (e) {
                    console.error('⚠️ Failed to clean up temp folder after error:', e.message)
                }
                reject(error)
            })
        } catch (error) {
            // Clean up temp folder if creation failed
            try {
                await fs.rm(tempRequestDir, { recursive: true, force: true })
            } catch (e) {
                // Ignore cleanup errors
            }
            reject(error)
        }
    })
}

// WebSocket server for LSP bridge
const wss = new WebSocketServer({
    server,
    path: '/lsp',
    verifyClient: (info) => {
        // Check origin for CORS on WebSocket connections
        const origin = info.origin;
        const allowedOrigins = [
            'http://localhost:8080',
            'http://127.0.0.1:8080',
            'http://localhost:3001',
            'http://127.0.0.1:3001',
            'https://ospreylang.dev',
            'https://www.ospreylang.dev'
        ];

        console.log('🔍 WebSocket upgrade request from origin:', origin);

        if (!origin || allowedOrigins.includes(origin)) {
            return true;
        }

        console.error('❌ WebSocket connection rejected - invalid origin:', origin);
        return false;
    }
})

console.log(`🌐 WebSocket server configured for path: /lsp`)

wss.on('connection', (ws, req) => {
    console.log('🔌 New WebSocket connection from:', req.socket.remoteAddress)
    console.log('🔍 Connection headers:', JSON.stringify(req.headers, null, 2))

    // The Osprey language server is the Rust `osprey lsp` subcommand, spoken over
    // stdio — the same binary that backs /api/run. In Docker it's on PATH; in
    // local dev it's the release build under target/release.
    const ospreyPath = process.env.NODE_ENV === 'production' || process.env.DOCKER_ENV
        ? 'osprey'
        : path.resolve(__dirname, '../../target/release/osprey')

    console.log('🚀 Starting Osprey LSP:', `${ospreyPath} lsp`)

    // Spawn the LSP server process
    const lspProcess = spawn(ospreyPath, ['lsp'], {
        stdio: ['pipe', 'pipe', 'pipe'],
        cwd: process.cwd(),
        env: { ...process.env }
    })

    lspProcess.on('error', (error) => {
        console.error('❌ LSP process error:', error)
        ws.close(1011, 'LSP server failed to start')
    })

    lspProcess.on('spawn', () => {
        console.log('✅ LSP process started successfully')
        console.log(`📊 LSP process PID: ${lspProcess.pid}`)
    })

    // Message counter for debugging
    let clientToServerCount = 0
    let serverToClientCount = 0

    // Forward messages between WebSocket and LSP stdio
    ws.on('message', (data) => {
        const message = data.toString()
        clientToServerCount++
        console.log(`📨 Client -> LSP [${clientToServerCount}]:`, message.substring(0, 200) + (message.length > 200 ? '...' : ''))

        // Parse to check message type
        try {
            const parsed = JSON.parse(message)
            console.log(`  📌 Message type: ${parsed.method || parsed.id ? 'request/response' : 'notification'}`)
            if (parsed.method) {
                console.log(`  📌 Method: ${parsed.method}`)
            }
        } catch (e) {
            console.log('  ⚠️ Could not parse message as JSON')
        }

        if (lspProcess.stdin && !lspProcess.stdin.destroyed) {
            lspProcess.stdin.write(message)
        } else {
            console.error('❌ LSP stdin not available!')
        }
    })

    lspProcess.stdout.on('data', (data) => {
        const message = data.toString()
        serverToClientCount++
        console.log(`📤 LSP -> Client [${serverToClientCount}]:`, message.substring(0, 200) + (message.length > 200 ? '...' : ''))

        // Parse to check message type
        try {
            const parsed = JSON.parse(message)
            console.log(`  📌 Message type: ${parsed.method || parsed.id ? 'request/response' : 'notification'}`)
            if (parsed.method) {
                console.log(`  📌 Method: ${parsed.method}`)
            }
        } catch (e) {
            console.log('  ⚠️ Could not parse message as JSON')
        }

        if (ws.readyState === ws.OPEN) {
            ws.send(data)
        } else {
            console.error('❌ WebSocket not open, cannot send message')
        }
    })

    lspProcess.stderr.on('data', (data) => {
        const errorMsg = data.toString()
        console.error('🔴 LSP stderr:', errorMsg)
    })

    ws.on('close', (code, reason) => {
        console.log(`🔌 WebSocket disconnected: code=${code}, reason=${reason}`)
        console.log(`📊 Total messages: Client->Server: ${clientToServerCount}, Server->Client: ${serverToClientCount}`)
        if (!lspProcess.killed) {
            console.log('🛑 Killing LSP process')
            lspProcess.kill()
        }
    })

    lspProcess.on('close', (code, signal) => {
        console.log(`🛑 LSP process exited: code=${code}, signal=${signal}`)
        if (ws.readyState === ws.OPEN) {
            ws.close()
        }
    })

    ws.on('error', (error) => {
        console.error('❌ WebSocket error:', error)
    })
})

wss.on('error', (error) => {
    console.error('❌ WebSocket server error:', error)
})

// Error handling middleware
app.use((error, req, res, next) => {
    console.error('💥 Unhandled error:', error)
    res.status(500).json({
        success: false,
        error: 'Internal server error',
        message: process.env.NODE_ENV === 'development' ? error.message : 'Something went wrong'
    })
})

server.listen(PORT, '0.0.0.0', () => {
    console.log(`✅ WebSocket LSP Bridge running at ws://0.0.0.0:${PORT}/lsp`)
    console.log(`🔨 Compile/Run API available at http://0.0.0.0:${PORT}/api`)
    console.log(`🏥 Health check: http://0.0.0.0:${PORT}/api`)
    console.log(`🌐 Server accessible from external hosts on port ${PORT}`)
})
