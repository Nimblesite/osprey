// Implements [IOS-HOST-ABI]: the scene's generated declarations are the Swift
// import boundary, as in every Apple example. ospgfx_abi.h adds the graphics
// contract every bridge shares: slot count, scales, uniforms and validation.
#include "ospgfx_abi.h"
#include "libOspreyApp.h"
