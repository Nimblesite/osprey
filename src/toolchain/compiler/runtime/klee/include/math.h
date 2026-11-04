// Mock <math.h> for symbolic execution [SYMEX-MOCK-HEADERS]. KLEE lowers the
// llvm.sqrt/exp/log intrinsics to these libm names and concretises floating
// point, so hunt_math.c implements them in plain C.
#ifndef OSP_HUNT_MATH_H
#define OSP_HUNT_MATH_H
#define INFINITY (__builtin_inff())
#define NAN (__builtin_nanf(""))
#define HUGE_VAL (__builtin_huge_val())
// Classification by comparison, not __builtin_isnan and friends: clang lowers
// those to llvm.is.fpclass, which KLEE does not execute.
static inline int isnan(double x) { return x != x; }
static inline int isinf(double x) { return __builtin_fabs(x) == __builtin_inf(); }
static inline int isfinite(double x) { return __builtin_fabs(x) <= 1.7976931348623157e308; }
static inline int signbit(double x) {
  union { double value; unsigned long long bits; } b = {x};
  return (int)(b.bits >> 63);
}
double fabs(double x);
double sqrt(double x);
double exp(double x);
double log(double x);
double log10(double x);
double log2(double x);
double pow(double base, double exponent);
double floor(double x);
double ceil(double x);
double trunc(double x);
double round(double x);
double fmod(double x, double y);
double sin(double x);
double cos(double x);
double tan(double x);
double tanh(double x);
double ldexp(double x, int exponent);
double frexp(double x, int *exponent);
#endif
