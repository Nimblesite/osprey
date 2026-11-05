// Flushing gcov counters from a child that will not run atexit handlers.
//
// A fatal guard and a scenario that owns process-global state are both shipped
// code, and the gcov gate cannot tell "never tested" from "untestable" -- it
// reads 0% for either. Both are executed in a FORKED CHILD here: a guard so the
// abort can be observed, a terminal or TAP scenario because it would otherwise
// leave the suite's own stdin, stdout and TAP counters wrecked for every test
// after it. Neither child runs `atexit`, so libgcov never wrote what they
// covered, and the honest response to a threshold is to make the measurement
// true rather than to exempt the lines.
//
// The instrumented test build defines OSP_COVERAGE_DUMP. A non-instrumented
// build leaves it unset because the coverage runtime is not linked there.
#ifndef OSPREY_TEST_GCOV_H
#define OSPREY_TEST_GCOV_H

#ifdef OSP_COVERAGE_DUMP
// A STRONG reference, and it has to be one. `dlsym` cannot find this symbol on
// ELF: measured on gcc 14 / glibc 2.36, `dlsym(RTLD_DEFAULT, "__gcov_dump")`
// answers NULL with AND without `-rdynamic`, because a symbol nothing
// references is never extracted from `libgcov.a` to begin with -- and a weak
// reference does not trigger archive extraction either. The previous spelling
// dumped NOTHING from any forked child on Linux while measuring correctly on
// macOS, which is a large part of why these suites read several points lower on
// the platform the gate actually runs on.
//
// Naming it at link time also removes a hazard rather than adding one: a
// SIGABRT handler that calls `dlsym` takes the loader's lock, which the
// interrupted thread may already hold.
extern void __gcov_dump(void);
#define OSP_GCOV_DUMP() __gcov_dump()
#else
// Nothing else about the child's behaviour depends on it, so a functional run
// is byte-identical.
#define OSP_GCOV_DUMP() ((void)0)
#endif

#endif // OSPREY_TEST_GCOV_H
