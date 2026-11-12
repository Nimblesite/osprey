// Implements [NATIVE-SHM-OWNERSHIP/PUBLISH]. Internal allocator API; host
// cutover and generated language owners are separate from this storage
// substrate.
#ifndef OSPREY_NATIVE_BRIDGE_H
#define OSPREY_NATIVE_BRIDGE_H
#include "native_abi.h"
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef struct OspNativeBridge OspNativeBridge;
#define OSP_NATIVE_OK 0
#define OSP_NATIVE_ERROR(name) (1 + TD_BridgeError_##name##_TAG)

// Status is zero or one plus the generated BridgeError discriminant. Outputs
// remain untouched on ordinary refusal; synchronization failure is fail-stop
// (the instance becomes unusable and keeps storage pinned). Destruction
// requires the application executor and all callers to have joined; a failed
// close never frees outstanding leases.
int osp_native_open(const td_ApplicationInstance *instance,
                    const char *fingerprint, int64_t bytes, int64_t chunks,
                    int64_t owners, OspNativeBridge **out);
int osp_native_create(OspNativeBridge *bridge, int64_t type, bool leaf,
                      const void *data, int64_t length,
                      const td_OwnedLease *children, size_t count,
                      td_OwnedLease *out);
int osp_native_retain(OspNativeBridge *bridge, const td_OwnedLease *lease,
                      td_OwnedLease *out);
int osp_native_release(OspNativeBridge *bridge, const td_OwnedLease *lease);
// The pointer is borrowed from this specific lease: do not release that owner
// during a borrow. Retain a distinct owner for work on another thread/GPU.
int osp_native_view(OspNativeBridge *bridge, const td_OwnedLease *lease,
                    int64_t type, int64_t offset, int64_t length,
                    const void **out);
// Success consumes the candidate owner. Only sealed, complete roots belong
// here; atomic application-state/command commits are not implemented by this
// API.
int osp_native_publish(OspNativeBridge *bridge, const td_OwnedLease *candidate,
                       int64_t *revision);
int osp_native_acquire(OspNativeBridge *bridge, td_OwnedLease *out,
                       int64_t *revision);
int osp_native_close(OspNativeBridge *bridge);
int osp_native_destroy(OspNativeBridge *bridge);
int osp_native_usage(OspNativeBridge *bridge, size_t *bytes, size_t *chunks,
                     size_t *owners);
#endif
