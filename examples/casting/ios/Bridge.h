#include <TargetConditionals.h>
#if TARGET_OS_SIMULATOR
#include "../build/ios-sim/libOspreyApp.h"
#else
#include "../build/ios/libOspreyApp.h"
#endif
