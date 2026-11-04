// Python-host callback ownership is defined by [PYTHON-CALL] and [PYTHON-OBJECT].
#ifndef OSPREY_PYTHON_BRIDGE_H
#define OSPREY_PYTHON_BRIDGE_H
#include <stdint.h>

typedef const char *(*PythonCall)(const char *, const char *, const char *);
typedef const char *(*PythonObject)(const char *, int64_t, const char *, const char *);

void osprey_python_install(PythonCall callback, PythonObject objects);
const char *osprey_python_call(const char *module, const char *name,
                             const char *args);
const char *osprey_python_object(const char *operation, int64_t object_id,
                               const char *name, const char *args);
#endif
