// Mock <stdio.h> for symbolic execution [SYMEX-MOCK-HEADERS]. Streams are the
// one stateful model [SYMEX-MOCK-STREAM]; formatting is hunt_format.c.
#ifndef OSP_HUNT_STDIO_H
#define OSP_HUNT_STDIO_H
#include <stdarg.h>
#include <stddef.h>
#include <sys/types.h>
typedef struct osp_hunt_file FILE;
extern FILE *osp_hunt_stdin;
extern FILE *osp_hunt_stdout;
extern FILE *osp_hunt_stderr;
#define stdin osp_hunt_stdin
#define stdout osp_hunt_stdout
#define stderr osp_hunt_stderr
#define EOF (-1)
#define BUFSIZ 1024
#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2
#define _IOFBF 0
#define _IOLBF 1
#define _IONBF 2
int printf(const char *format, ...);
int fprintf(FILE *stream, const char *format, ...);
int sprintf(char *out, const char *format, ...);
int snprintf(char *out, size_t cap, const char *format, ...);
int vsnprintf(char *out, size_t cap, const char *format, va_list arguments);
int vfprintf(FILE *stream, const char *format, va_list arguments);
int vprintf(const char *format, va_list arguments);
int puts(const char *text);
int fputs(const char *text, FILE *stream);
int putchar(int c);
int fputc(int c, FILE *stream);
int putc(int c, FILE *stream);
int getchar(void);
int fgetc(FILE *stream);
int getc(FILE *stream);
char *fgets(char *out, int cap, FILE *stream);
size_t fread(void *out, size_t size, size_t count, FILE *stream);
size_t fwrite(const void *data, size_t size, size_t count, FILE *stream);
FILE *fopen(const char *path, const char *mode);
int fclose(FILE *stream);
int fflush(FILE *stream);
int ferror(FILE *stream);
int feof(FILE *stream);
void clearerr(FILE *stream);
int fseek(FILE *stream, long offset, int whence);
int fseeko(FILE *stream, off_t offset, int whence);
long ftell(FILE *stream);
off_t ftello(FILE *stream);
void rewind(FILE *stream);
int setvbuf(FILE *stream, char *buffer, int mode, size_t size);
int remove(const char *path);
int rename(const char *from, const char *to);
FILE *popen(const char *command, const char *mode);
int pclose(FILE *stream);
#endif
