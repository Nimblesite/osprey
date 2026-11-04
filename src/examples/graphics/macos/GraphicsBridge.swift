import AppKit
import Metal
import QuartzCore

// Implements [IOS-HOST-IMPORTS] and [IOS-TARGET-MACOS]: the six functions
// `base/base.osp` declares with `extern`, defined in Swift with `@_cdecl`. A
// Cocoa window backed by a Metal layer; a fragment shader compiled from source
// shades every pixel from the slots Osprey pushes. The slot count, scales,
// uniform block and validation come from ospgfx_abi.h through Bridge.h, the
// same header the Direct3D 12 bridge includes.

/// Ends every scene after this many frames when set, as if its window closed,
/// so a smoke run can measure a scene without a person to close it.
private let frameLimit = ProcessInfo.processInfo.environment["OSPREY_GFX_FRAMES"].flatMap { Int($0) }

/// One open window. Osprey holds its address as a `Ptr` and never looks inside.
private final class Scene: NSObject, NSWindowDelegate {
    let window: NSWindow
    let layer: CAMetalLayer
    let queue: MTLCommandQueue
    let pipeline: MTLRenderPipelineState
    let openedAt = now()
    var uniforms = OspGfxUniforms()
    var frames = 0
    var closed = false

    init(window: NSWindow, layer: CAMetalLayer, queue: MTLCommandQueue, pipeline: MTLRenderPipelineState) {
        self.window = window
        self.layer = layer
        self.queue = queue
        self.pipeline = pipeline
        super.init()
        window.delegate = self
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        closed = true
        return true
    }

    /// Resolution and elapsed time are the uniforms the host fills in: only the
    /// drawable knows its size at this display's density, and the clock moved.
    func encode(_ drawable: CAMetalDrawable) {
        uniforms.res = (Float(layer.drawableSize.width), Float(layer.drawableSize.height))
        uniforms.time = Float(now() - openedAt)
        let pass = MTLRenderPassDescriptor()
        pass.colorAttachments[0].texture = drawable.texture
        pass.colorAttachments[0].loadAction = .clear
        pass.colorAttachments[0].storeAction = .store
        pass.colorAttachments[0].clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 1)
        guard let commands = queue.makeCommandBuffer(),
            let encoder = commands.makeRenderCommandEncoder(descriptor: pass)
        else { return }
        var frame = uniforms
        encoder.setRenderPipelineState(pipeline)
        encoder.setFragmentBytes(&frame, length: MemoryLayout<OspGfxUniforms>.size, index: 0)
        encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
        encoder.endEncoding()
        commands.present(drawable)
        commands.commit()
    }
}

/// Open scenes by the address Osprey holds, so a closed or unknown handle is
/// refused rather than dereferenced.
nonisolated(unsafe) private var scenes: [UnsafeMutableRawPointer: Scene] = [:]

private func scene(_ handle: UnsafeMutableRawPointer?) -> Scene? {
    handle.flatMap { scenes[$0] }
}

/// Seconds from a monotonic clock, so a reset wall clock cannot rewind a scene.
private func now() -> Double {
    Double(clock_gettime_nsec_np(CLOCK_MONOTONIC)) / 1e9
}

private func fail(_ message: String) {
    FileHandle.standardError.write(Data((message + "\n").utf8))
}

/// A regular foreground app, so the window takes focus when launched from a terminal.
private let application: NSApplication = {
    let app = NSApplication.shared
    _ = app.setActivationPolicy(.regular)
    app.finishLaunching()
    app.activate(ignoringOtherApps: true)
    return app
}()

/// Compiles the library Osprey named and selects the fragment entry the scene
/// asked for. Scenes share one library and differ only in that entry: a library
/// compiled from source at run time has no include path, so sharing a shader
/// means sharing one file. A shader that will not compile fails the open with
/// its diagnostics instead of presenting a blank window.
private func pipeline(_ device: MTLDevice, _ shaderPath: String, _ entry: String) -> MTLRenderPipelineState? {
    guard let source = try? String(contentsOfFile: shaderPath, encoding: .utf8) else {
        fail("ospgfx: cannot read shader \(shaderPath)")
        return nil
    }
    let descriptor = MTLRenderPipelineDescriptor()
    do {
        let library = try device.makeLibrary(source: source, options: nil)
        descriptor.vertexFunction = library.makeFunction(name: OSP_GFX_VERTEX_ENTRY)
        descriptor.fragmentFunction = library.makeFunction(name: entry)
    } catch {
        fail("ospgfx: shader failed to compile:\n\(error.localizedDescription)")
        return nil
    }
    descriptor.colorAttachments[0].pixelFormat = .bgra8Unorm
    guard descriptor.vertexFunction != nil, descriptor.fragmentFunction != nil else {
        fail("ospgfx: shader needs osp_vertex and \(entry)")
        return nil
    }
    do {
        return try device.makeRenderPipelineState(descriptor: descriptor)
    } catch {
        fail("ospgfx: pipeline failed: \(error.localizedDescription)")
        return nil
    }
}

/// A window `size` points large, drawn at the display's full backing density.
private func window(_ size: NSSize, _ title: String, _ device: MTLDevice) -> (NSWindow, CAMetalLayer) {
    let frame = NSRect(origin: .zero, size: size)
    let window = NSWindow(
        contentRect: frame, styleMask: [.titled, .closable, .miniaturizable], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.title = title
    let layer = CAMetalLayer()
    layer.device = device
    layer.pixelFormat = .bgra8Unorm
    layer.framebufferOnly = true
    layer.contentsScale = window.backingScaleFactor
    layer.drawableSize = CGSize(width: size.width * window.backingScaleFactor, height: size.height * window.backingScaleFactor)
    let view = NSView(frame: frame)
    view.wantsLayer = true
    view.layer = layer
    window.contentView = view
    window.center()
    window.makeKeyAndOrderFront(nil)
    return (window, layer)
}

/// `scale` magnifies the design resolution into window points; `fragmentName`
/// picks one entry out of the shared library at `shaderPath`.
@_cdecl("osp_gfx_open")
func ospGfxOpen(
    _ width: Int64, _ height: Int64, _ scale: Int64, _ title: UnsafePointer<CChar>?,
    _ shaderPath: UnsafePointer<CChar>?, _ fragmentName: UnsafePointer<CChar>?
) -> UnsafeMutableRawPointer? {
    guard osp_gfx_extent_ok(width, height) != 0 else { return nil }
    let magnified = CGFloat(osp_gfx_clamp_scale(scale))
    _ = application
    guard let device = MTLCreateSystemDefaultDevice() else {
        fail("ospgfx: no Metal device")
        return nil
    }
    let entry = String(cString: osp_gfx_fragment_entry(fragmentName))
    guard let pipeline = pipeline(device, shaderPath.map { String(cString: $0) } ?? "", entry),
        let queue = device.makeCommandQueue()
    else { return nil }
    let size = NSSize(width: CGFloat(width) * magnified, height: CGFloat(height) * magnified)
    let (window, layer) = window(size, title.map { String(cString: $0) } ?? "Osprey", device)
    let opened = Scene(window: window, layer: layer, queue: queue, pipeline: pipeline)
    let handle = Unmanaged.passUnretained(opened).toOpaque()
    scenes[handle] = opened
    return handle
}

/// Pushes one moving scene parameter, in 4096ths; the shader sees a float.
@_cdecl("osp_gfx_set")
func ospGfxSet(_ window: UnsafeMutableRawPointer?, _ slot: Int64, _ value: Int64) -> Int64 {
    guard let scene = scene(window) else { return 0 }
    return osp_gfx_slot_store(&scene.uniforms, slot, value, OSP_GFX_FIXED_ONE)
}

/// Pushes one authored tuning constant, in thousandths, which divides back to
/// the float the same literal produced when it lived in the shader.
@_cdecl("osp_gfx_set_milli")
func ospGfxSetMilli(_ window: UnsafeMutableRawPointer?, _ slot: Int64, _ thousandths: Int64) -> Int64 {
    guard let scene = scene(window) else { return 0 }
    return osp_gfx_slot_store(&scene.uniforms, slot, thousandths, OSP_GFX_MILLI_ONE)
}

/// Shades every pixel from the current slots and presents. Returns 1 while the
/// window is open, 0 once it has closed.
@_cdecl("osp_gfx_draw")
func ospGfxDraw(_ window: UnsafeMutableRawPointer?) -> Int64 {
    guard let scene = scene(window), !scene.closed else { return 0 }
    if let frameLimit, scene.frames >= frameLimit { scene.closed = true }
    guard !scene.closed else { return 0 }
    autoreleasepool {
        if let drawable = scene.layer.nextDrawable() { scene.encode(drawable) }
        while let event = application.nextEvent(matching: .any, until: nil, inMode: .default, dequeue: true) {
            application.sendEvent(event)
        }
    }
    scene.frames += 1
    return scene.closed ? 0 : 1
}

/// Milliseconds since the window opened, so animation runs on wall-clock time.
@_cdecl("osp_gfx_ticks")
func ospGfxTicks(_ window: UnsafeMutableRawPointer?) -> Int64 {
    scene(window).map { osp_gfx_elapsed_ms($0.openedAt, now()) } ?? 0
}

/// Closes the window and forgets the handle; a second close returns 0.
@_cdecl("osp_gfx_close")
func ospGfxClose(_ window: UnsafeMutableRawPointer?) -> Int64 {
    guard let window, let closing = scenes.removeValue(forKey: window) else { return 0 }
    closing.window.delegate = nil
    closing.window.orderOut(nil)
    return 1
}
