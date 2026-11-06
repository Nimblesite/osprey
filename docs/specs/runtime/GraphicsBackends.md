# Graphics Backends

**Status:** partially implemented. The Metal backend renders the three graphics scenes on macOS; the Direct3D 12 bridge compiles, links and draws every scene in software on Windows (2026-10-28). No hardware adapter has drawn a scene yet, no frame has been compared with the Mac's at matched uniforms, and the convergence with `gpu*` is not implemented. [Plan 0023](../../plans/0023-gpu.md) owns delivery; until then [src/osprey/examples/graphics/README.md](../../../src/osprey/examples/graphics/README.md) is the operating contract.

Graphics in Osprey is a host boundary, not the `gpu*` compute surface ([0034](0034-GPUComputation.md)): an Osprey host program drives a window through six extern C functions, and a hand-written fragment shader shades every pixel from the parameter slots Osprey pushes into it. The Osprey sources are byte-identical on every platform.

## One ABI declaration — [GFX-ABI]

The Osprey-facing C contract is declared **once**, in `ospgfx_abi.h`: `OSP_GFX_SLOTS`, the two fixed-point scales, the `OspGfxUniforms` block, the window-extent and scale validation, the fragment-entry default, the slot store and the elapsed-time helper. Both bridges include it and restate none of it; the drift guard fails if either bridge grows its own `#define OSP_GFX_SLOTS` or `typedef struct OspGfxUniforms` again.

The uniform-slot protocol both bridges implement is declared once, in `src/osprey/examples/graphics/ospgfx_abi.h`, and a kernel's leading uniform parameters are exactly a slot vector — that header is the seam the `gpu*` convergence attaches to ([GPU-KERNEL-EXTRACT](0034-GPUComputation.md#kernel-extraction--gpu-kernel-extract)).

## Shader library — [GFX-SHADER-LIB]

Pixel-for-pixel identity across platforms is **not** claimed and must not be. The arithmetic is transcribed operation for operation, but the shader compilers fold, reassociate and schedule independently: "indistinguishable", never "bit-equal".

Authored grade lives in Osprey, not the shader. An offscreen A/B render at matched uniforms was byte-identical when the grade moved out of the shader; that is the bar the `gpu*` convergence inherits.

## Uniform transcription hazards — [GFX-UNIFORMS]

Two transcription hazards are load-bearing, and each carries a dedicated test and comment because each compiles cleanly when wrong.

- **HLSL cbuffer packing.** A `float slot[24]` inside an HLSL constant buffer gets one 16-byte register *per element* and would read `slot[1]` out of the bridge's `slot[4]`. It is declared `float4 uniformSlotPack[OSP_GFX_SLOTS / 4]` and unpacked once by `loadUniforms()`; a `#if (OSP_GFX_SLOTS % 4) != 0 / #error` guards the divisibility, and `the_hlsl_constant_buffer_cannot_quietly_unpack_itself` guards the guard.
- **Matrix convention.** Metal's `float2x2` takes **columns**; HLSL's takes **rows** and needs `mul`. `rotationFromTurns` is transposed accordingly; get it wrong and every scene spins backwards at full frame rate.

Only a frame compared with the Mac's at matched uniforms settles either hazard; that comparison is not implemented yet.
