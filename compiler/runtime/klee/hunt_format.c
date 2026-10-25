// The printf family for the mock libc [SYMEX-MOCK-STATELESS]. One engine
// formats into a byte callback; snprintf counts into a buffer, printf and
// fprintf count into the sink. Supports flags `-+ 0#`, width and precision
// (including `*`), lengths `hh h l ll z j t`, and conversions
// `d i u o x X c s p % f F e E g G`. Floating point goes through the exact
// decimal expansion in hunt_decimal.c, so the digits match the host libc's
// and a program that branches on `toString(x)` explores the same paths.
#include "hunt_mock.h"

#include <math.h>
#include <stdio.h>
#include <string.h>

typedef struct {
  int left;
  int zero;
  int plus;
  int space;
  int alt;
  int width;
  int precision;
  int length;
  char conversion;
} spec;

typedef struct {
  void (*emit)(void *target, char byte);
  void *target;
  int count;
} sink;

typedef struct {
  char *buffer;
  size_t cap;
  size_t used;
} bounded;

// A `%f` body is at most 309 integer digits, the point, the precision and
// an exponent suffix; precision is capped so the body always fits.
enum { FLOAT_PRECISION_MAX = 400, FLOAT_BODY_MAX = 309 + 1 + FLOAT_PRECISION_MAX + 8 };
enum { DIGITS_MAX = 24, LENGTH_LONG = 1, LENGTH_LONG_LONG = 2, LENGTH_SIZE = 3, LENGTH_SHORT = -1, LENGTH_CHAR = -2 };

static void put(sink *out, char byte) {
  out->emit(out->target, byte);
  out->count++;
}

static void put_text(sink *out, const char *text, size_t length) {
  for (size_t i = 0; i < length; i++) {
    put(out, text[i]);
  }
}

static void pad(sink *out, int count, char byte) {
  for (int i = 0; i < count; i++) {
    put(out, byte);
  }
}

static const char *parse_flags(const char *format, spec *s) {
  for (;; format++) {
    switch (*format) {
    case '-': s->left = 1; break;
    case '0': s->zero = 1; break;
    case '+': s->plus = 1; break;
    case ' ': s->space = 1; break;
    case '#': s->alt = 1; break;
    default: return format;
    }
  }
}

static const char *parse_number(const char *format, int *value, va_list *arguments) {
  if (*format == '*') {
    *value = va_arg(*arguments, int);
    return format + 1;
  }
  *value = 0;
  while (*format >= '0' && *format <= '9') {
    *value = *value * 10 + (*format - '0');
    format++;
  }
  return format;
}

static const char *parse_length(const char *format, spec *s) {
  switch (*format) {
  case 'l': s->length = format[1] == 'l' ? LENGTH_LONG_LONG : LENGTH_LONG; return format + (format[1] == 'l' ? 2 : 1);
  case 'h': s->length = format[1] == 'h' ? LENGTH_CHAR : LENGTH_SHORT; return format + (format[1] == 'h' ? 2 : 1);
  case 'z': case 'j': case 't': s->length = LENGTH_SIZE; return format + 1;
  default: return format;
  }
}

static const char *parse_spec(const char *format, spec *s, va_list *arguments) {
  spec blank = {0, 0, 0, 0, 0, 0, -1, 0, 0};
  *s = blank;
  format = parse_flags(format, s);
  format = parse_number(format, &s->width, arguments);
  if (s->width < 0) {
    s->left = 1;
    s->width = -s->width;
  }
  if (*format == '.') {
    format = parse_number(format + 1, &s->precision, arguments);
  }
  format = parse_length(format, s);
  s->conversion = *format;
  return *format ? format + 1 : format;
}

// Writes `body` right-aligned in `width`, with the sign before any zero fill.
static void emit_field(sink *out, const spec *s, const char *prefix, const char *body, int body_length, int zero_fill) {
  int prefix_length = (int)strlen(prefix);
  int filler = s->width - prefix_length - body_length;
  if (!s->left && !zero_fill) {
    pad(out, filler, ' ');
  }
  put_text(out, prefix, (size_t)prefix_length);
  if (!s->left && zero_fill) {
    pad(out, filler, '0');
  }
  put_text(out, body, (size_t)body_length);
  if (s->left) {
    pad(out, filler, ' ');
  }
}

// Compare the original number with concrete powers instead of repeatedly
// asking the solver whether a symbolic quotient is zero.
static int digit_length(unsigned long long value, unsigned base) {
  int length = 0;
  unsigned long long threshold = 1;
  do {
    length++;
    if (threshold > UINT64_MAX / base) break;
    threshold *= base;
  } while (value >= threshold);
  return length;
}

// Digits of `value` in `base`, written backwards from the end of `digits`.
static struct { unsigned long long value; char digits[20]; size_t count; } decimal_cache;

void osp_hunt_cache_decimal(uint64_t value, const char *digits, size_t count) {
  decimal_cache.count = 0;
  if (count > 19) return;
  decimal_cache.value = value;
  decimal_cache.count = count == 0 ? 1 : count;
  if (count == 0) decimal_cache.digits[0] = '0';
  else memcpy(decimal_cache.digits, digits, count);
}

static int cached_decimal(unsigned long long value, char *digits) {
  if (decimal_cache.count == 0 || value != decimal_cache.value) return 0;
  memcpy(digits + DIGITS_MAX - decimal_cache.count, decimal_cache.digits, decimal_cache.count);
  return (int)decimal_cache.count;
}

static int unsigned_digits(unsigned long long value, unsigned base, int upper, char *digits) {
  int cached = base == 10 ? cached_decimal(value, digits) : 0;
  if (cached != 0) return cached;
  int length = digit_length(value, base);
  unsigned long long divisor = 1;
  for (int i = 0; i < length; i++) {
    unsigned digit = (unsigned)((value / divisor) % base);
    digits[DIGITS_MAX - 1 - i] = (char)(digit < 10 ? '0' + digit : (upper ? 'A' : 'a') + digit - 10);
    divisor *= base;
  }
  return length;
}

static long long signed_argument(const spec *s, va_list *arguments) {
  switch (s->length) {
  case LENGTH_LONG: return va_arg(*arguments, long);
  case LENGTH_LONG_LONG: return va_arg(*arguments, long long);
  case LENGTH_SIZE: return (long long)va_arg(*arguments, size_t);
  case LENGTH_SHORT: return (short)va_arg(*arguments, int);
  case LENGTH_CHAR: return (signed char)va_arg(*arguments, int);
  default: return va_arg(*arguments, int);
  }
}

static unsigned long long unsigned_argument(const spec *s, va_list *arguments) {
  switch (s->length) {
  case LENGTH_LONG: return va_arg(*arguments, unsigned long);
  case LENGTH_LONG_LONG: return va_arg(*arguments, unsigned long long);
  case LENGTH_SIZE: return va_arg(*arguments, size_t);
  case LENGTH_SHORT: return (unsigned short)va_arg(*arguments, unsigned);
  case LENGTH_CHAR: return (unsigned char)va_arg(*arguments, unsigned);
  default: return va_arg(*arguments, unsigned);
  }
}

static void emit_integer(sink *out, const spec *s, unsigned long long magnitude, const char *sign, unsigned base, int upper) {
  char digits[DIGITS_MAX];
  int length = unsigned_digits(magnitude, base, upper, digits);
  if (s->precision == 0 && magnitude == 0) {
    length = 0;
  }
  char body[DIGITS_MAX * 2];
  int zeros = s->precision > length ? s->precision - length : 0;
  memset(body, '0', (size_t)zeros);
  memcpy(body + zeros, digits + DIGITS_MAX - length, (size_t)length);
  emit_field(out, s, sign, body, zeros + length, s->zero && s->precision < 0);
}

static void emit_signed(sink *out, const spec *s, va_list *arguments) {
  long long value = signed_argument(s, arguments);
  unsigned long long magnitude = value < 0 ? 0ULL - (unsigned long long)value : (unsigned long long)value;
  const char *sign = value < 0 ? "-" : s->plus ? "+" : s->space ? " " : "";
  emit_integer(out, s, magnitude, sign, 10, 0);
}

static void emit_unsigned(sink *out, const spec *s, va_list *arguments) {
  unsigned long long value = unsigned_argument(s, arguments);
  unsigned base = s->conversion == 'o' ? 8 : s->conversion == 'u' ? 10 : 16;
  int upper = s->conversion == 'X';
  const char *prefix = s->alt && value != 0 ? (base == 8 ? "0" : upper ? "0X" : "0x") : "";
  emit_integer(out, s, value, prefix, base, upper);
}

static void emit_pointer(sink *out, spec *s, va_list *arguments) {
  unsigned long long value = (unsigned long long)(uintptr_t)va_arg(*arguments, void *);
  s->zero = 0;
  emit_integer(out, s, value, value == 0 ? "" : "0x", 16, 0);
}

static void emit_string(sink *out, const spec *s, va_list *arguments) {
  const char *text = va_arg(*arguments, const char *);
  if (text == NULL) {
    text = "(null)";
  }
  size_t length = strlen(text);
  if (s->precision >= 0 && (size_t)s->precision < length) {
    length = (size_t)s->precision;
  }
  emit_field(out, s, "", text, (int)length, 0);
}

static void emit_char(sink *out, const spec *s, va_list *arguments) {
  char byte = (char)va_arg(*arguments, int);
  emit_field(out, s, "", &byte, 1, 0);
}

// Digit at `index` of the expansion; positions outside it are zeros.
static char digit_at(const struct osp_hunt_decimal *d, int index) {
  return index >= 0 && index < d->count ? d->digits[index] : '0';
}

static int exponent_suffix(int exponent, int upper, char *body, int at) {
  body[at++] = upper ? 'E' : 'e';
  body[at++] = exponent < 0 ? '-' : '+';
  int magnitude = exponent < 0 ? -exponent : exponent;
  if (magnitude >= 100) {
    body[at++] = (char)('0' + magnitude / 100);
  }
  body[at++] = (char)('0' + magnitude / 10 % 10);
  body[at++] = (char)('0' + magnitude % 10);
  return at;
}

// `%f` body: the digits before the point, then `precision` after it. The
// expansion must already be rounded at `point + precision`.
static int fixed_body(const struct osp_hunt_decimal *d, int precision, int alt, char *body) {
  int length = 0;
  if (d->point <= 0) {
    body[length++] = '0';
  }
  for (int i = 0; i < d->point; i++) {
    body[length++] = digit_at(d, i);
  }
  if (precision > 0 || alt) {
    body[length++] = '.';
  }
  for (int i = 0; i < precision; i++) {
    body[length++] = digit_at(d, d->point + i);
  }
  return length;
}

// `%e` body: one digit, the point, `precision` digits and the exponent. The
// expansion must already be rounded to `precision + 1` digits.
static int scientific_body(const struct osp_hunt_decimal *d, int precision, int alt, int upper, char *body) {
  int length = 0;
  body[length++] = digit_at(d, 0);
  if (precision > 0 || alt) {
    body[length++] = '.';
  }
  for (int i = 1; i <= precision; i++) {
    body[length++] = digit_at(d, i);
  }
  return exponent_suffix(d->count == 0 ? 0 : d->point - 1, upper, body, length);
}

// Drops trailing zeros (and a bare point) from the fraction of a `%g` body.
static int strip_zeros(char *body, int length, int tail_start) {
  int point = -1;
  for (int i = 0; i < tail_start; i++) {
    if (body[i] == '.') {
      point = i;
    }
  }
  if (point < 0) {
    return length;
  }
  int end = tail_start;
  while (end > point + 1 && body[end - 1] == '0') {
    end--;
  }
  if (end == point + 1) {
    end--;
  }
  memmove(body + end, body + tail_start, (size_t)(length - tail_start));
  return length - (tail_start - end);
}

// `%g`: `%e` when the exponent is below -4 or at least the precision, else `%f`.
static int general_body(struct osp_hunt_decimal *d, int precision, int alt, int upper, char *body) {
  if (precision == 0) {
    precision = 1;
  }
  osp_hunt_decimal_round(d, precision);
  int exponent = d->count == 0 ? 0 : d->point - 1;
  int length;
  int tail;
  if (exponent < -4 || exponent >= precision) {
    length = scientific_body(d, precision - 1, alt, upper, body);
    tail = length;
    while (tail > 0 && body[tail - 1] != 'e' && body[tail - 1] != 'E') {
      tail--;
    }
    tail--;
  } else {
    length = fixed_body(d, precision - 1 - exponent, alt, body);
    tail = length;
  }
  return alt ? length : strip_zeros(body, length, tail);
}

static int float_body(const spec *s, double magnitude, int precision, int upper, char *body) {
  struct osp_hunt_decimal d;
  osp_hunt_decimal(magnitude, &d);
  if (s->conversion == 'f' || s->conversion == 'F') {
    osp_hunt_decimal_round(&d, d.point + precision);
    return fixed_body(&d, precision, s->alt, body);
  }
  if (s->conversion == 'e' || s->conversion == 'E') {
    osp_hunt_decimal_round(&d, precision + 1);
    return scientific_body(&d, precision, s->alt, upper, body);
  }
  return general_body(&d, precision, s->alt, upper, body);
}

static void emit_nonfinite(sink *out, const spec *s, double value, const char *sign) {
  int upper = s->conversion == 'F' || s->conversion == 'E' || s->conversion == 'G';
  const char *body = isnan(value) ? (upper ? "NAN" : "nan") : (upper ? "INF" : "inf");
  emit_field(out, s, sign, body, 3, 0);
}

static void emit_float(sink *out, const spec *s, va_list *arguments) {
  double value = va_arg(*arguments, double);
  const char *sign = signbit(value) ? "-" : s->plus ? "+" : s->space ? " " : "";
  if (!isfinite(value)) {
    emit_nonfinite(out, s, value, sign);
    return;
  }
  int precision = s->precision < 0 ? 6 : s->precision;
  if (precision > FLOAT_PRECISION_MAX) {
    precision = FLOAT_PRECISION_MAX;
  }
  int upper = s->conversion == 'F' || s->conversion == 'E' || s->conversion == 'G';
  char body[FLOAT_BODY_MAX];
  int length = float_body(s, fabs(value), precision, upper, body);
  emit_field(out, s, sign, body, length, s->zero);
}

static void emit_conversion(sink *out, spec *s, va_list *arguments) {
  switch (s->conversion) {
  case 'd': case 'i': emit_signed(out, s, arguments); break;
  case 'u': case 'o': case 'x': case 'X': emit_unsigned(out, s, arguments); break;
  case 'c': emit_char(out, s, arguments); break;
  case 's': emit_string(out, s, arguments); break;
  case 'p': emit_pointer(out, s, arguments); break;
  case 'f': case 'F': case 'e': case 'E': case 'g': case 'G': emit_float(out, s, arguments); break;
  case '%': put(out, '%'); break;
  default: break;
  }
}

int osp_hunt_vformat(void (*emit)(void *target, char byte), void *target, const char *format, va_list arguments) {
  osp_hunt_note(OSP_HUNT_CALL_FORMAT);
  sink out = {emit, target, 0};
  va_list cursor;
  va_copy(cursor, arguments);
  while (*format) {
    if (*format != '%') {
      put(&out, *format++);
      continue;
    }
    spec s;
    format = parse_spec(format + 1, &s, &cursor);
    emit_conversion(&out, &s, &cursor);
  }
  va_end(cursor);
  return out.count;
}

static void emit_bounded(void *target, char byte) {
  bounded *out = target;
  if (out->used + 1 < out->cap) {
    out->buffer[out->used] = byte;
  }
  out->used++;
}

static void emit_sink(void *target, char byte) {
  (void)target;
  osp_hunt_sink(&byte, 1);
}

int vsnprintf(char *out, size_t cap, const char *format, va_list arguments) {
  bounded target = {out, cap, 0};
  int length = osp_hunt_vformat(emit_bounded, &target, format, arguments);
  if (cap > 0) {
    out[target.used < cap ? target.used : cap - 1] = '\0';
  }
  return length;
}

int snprintf(char *out, size_t cap, const char *format, ...) {
  va_list arguments;
  va_start(arguments, format);
  int length = vsnprintf(out, cap, format, arguments);
  va_end(arguments);
  return length;
}

int sprintf(char *out, const char *format, ...) {
  va_list arguments;
  va_start(arguments, format);
  int length = vsnprintf(out, (size_t)-1, format, arguments);
  va_end(arguments);
  return length;
}

int vfprintf(FILE *stream, const char *format, va_list arguments) {
  osp_hunt_note(OSP_HUNT_CALL_PRINTF);
  if (!stream->writable) {
    stream->error = 1;
    return -1;
  }
  return osp_hunt_vformat(emit_sink, NULL, format, arguments);
}

int vprintf(const char *format, va_list arguments) { return vfprintf(osp_hunt_stdout, format, arguments); }

int fprintf(FILE *stream, const char *format, ...) {
  va_list arguments;
  va_start(arguments, format);
  int length = vfprintf(stream, format, arguments);
  va_end(arguments);
  return length;
}

int printf(const char *format, ...) {
  va_list arguments;
  va_start(arguments, format);
  int length = vfprintf(osp_hunt_stdout, format, arguments);
  va_end(arguments);
  return length;
}
