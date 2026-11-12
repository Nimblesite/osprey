// The shared-memory view host's C target. `view.sh` copies the application's
// generated header and the two bridge headers into `include/`; including the
// module header here gives Swift one module with every declaration it needs.
#include "include/OspreyViewApp.h"
