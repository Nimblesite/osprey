// The whole C surface of the view host ([NATIVE-SHM-VIEW]): the application's
// generated exports, the bridge that owns the frame it writes, and the layouts
// the host reads in place. `view.sh` and `run.sh` copy the three headers below
// beside this file, so the host never keeps a second copy of them.
#pragma once
#include "libOspreyApp.h"
#include "native_bridge.h"
#include "native_abi.h"
