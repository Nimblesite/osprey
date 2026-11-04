// Python-hosted JSON calls. The host owns the callback and its temporary reply;
// copy each reply into Osprey's allocator before returning. [PYTHON-CALL]
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "python_bridge.h"
#include "memory_hooks.h"

static PythonCall python_call;
static PythonObject python_object;

static void allocation_failure(const char *reason) {
  fputs(reason, stderr);
  abort();
}

static const char *copy_reply(const char *reply) {
  if (!reply) {
    allocation_failure("Osprey Python callback returned a null reply\n");
  }
  size_t size = strlen(reply) + 1;
  if (size > INT64_MAX) {
    allocation_failure("Osprey Python reply exceeds the allocator limit\n");
  }
  char *copy = osp_alloc_tagged((int64_t)size, 0);
  if (!copy) {
    allocation_failure("Osprey Python reply allocation failed\n");
  }
  memcpy(copy, reply, size);
  return copy;
}

void osprey_python_install(PythonCall callback, PythonObject objects) {
  python_call = callback;
  python_object = objects;
}

static const char *missing_host =
    "{\"error\":{\"type\":\"RuntimeError\",\"message\":\"Python host is not installed\"}}";

const char *osprey_python_call(const char *module, const char *name,
                             const char *args) {
  return copy_reply(python_call ? python_call(module, name, args) : missing_host);
}

const char *osprey_python_object(const char *operation, int64_t object_id,
                               const char *name, const char *args) {
  return copy_reply(python_object ? python_object(operation, object_id, name, args)
                                  : missing_host);
}
