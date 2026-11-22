// file_dirs_win.h — listDir, kindOf and makeDir on Windows, included once by
// file_runtime.c in place of the POSIX dirent/stat branch. It uses the ANSI
// Win32 calls so a path means exactly what it means to readFile/writeFile's
// fopen. Implements [BUILTIN-FILE-LISTDIR], [BUILTIN-FILE-KIND],
// [BUILTIN-FILE-MAKEDIR] with the same contract as the POSIX branch.
#ifndef OSPREY_FILE_DIRS_WIN_H
#define OSPREY_FILE_DIRS_WIN_H

// The errno a Win32 failure reports through osp_io_error_set, so the
// refusal text reads the same as on POSIX hosts.
static int win_errno(DWORD error) {
  switch (error) {
  case ERROR_FILE_NOT_FOUND:
  case ERROR_PATH_NOT_FOUND:
  case ERROR_INVALID_NAME:
    return ENOENT;
  case ERROR_ACCESS_DENIED:
  case ERROR_SHARING_VIOLATION:
    return EACCES;
  case ERROR_ALREADY_EXISTS:
  case ERROR_FILE_EXISTS:
    return EEXIST;
  case ERROR_DIRECTORY:
    return ENOTDIR;
  case ERROR_NOT_ENOUGH_MEMORY:
  case ERROR_OUTOFMEMORY:
    return ENOMEM;
  case ERROR_FILENAME_EXCED_RANGE:
    return ENAMETOOLONG;
  default:
    return EIO;
  }
}

// The attributes of `path`, or INVALID_FILE_ATTRIBUTES with *err set. An
// empty path is ENOENT, as opendir("") is, never the drive root.
static DWORD win_attributes(const char *path, int *err) {
  DWORD attributes = path[0] == '\0' ? INVALID_FILE_ATTRIBUTES : GetFileAttributesA(path);
  *err = attributes != INVALID_FILE_ATTRIBUTES ? 0 : path[0] == '\0' ? ENOENT : win_errno(GetLastError());
  return attributes;
}

// A reparse point (symlink, junction) is never a directory to a walker, the
// lstat rule of the POSIX branch: a link to an ancestor can never recurse.
static int win_walkable_dir(DWORD attributes) {
  return (attributes & FILE_ATTRIBUTE_DIRECTORY) != 0 && (attributes & FILE_ATTRIBUTE_REPARSE_POINT) == 0;
}

// One found entry's full path, a trailing separator marking a directory.
// Returns 0, or the errno that refuses the entry.
static int win_entry_path(char *out, size_t size, const char *dir, const WIN32_FIND_DATAA *found) {
  size_t length = osp_path_child(out, size, dir, found->cFileName);
  if (length == 0 || length + 1 >= size) {
    return ENAMETOOLONG;
  }
  if (win_walkable_dir(found->dwFileAttributes)) {
    out[length] = OSP_PATH_SEP;
    out[length + 1] = '\0';
  }
  return 0;
}

// Pushes `found` and every later entry of `search` onto `builder`, skipping
// "." and ".."; 0, or the errno that stopped the walk.
static int win_push_entries(HANDLE search, WIN32_FIND_DATAA *found, const char *path,
                            OspreyListBuilder *builder) {
  char joined[OSP_PATH_MAX];
  do {
    if (strcmp(found->cFileName, ".") == 0 || strcmp(found->cFileName, "..") == 0) {
      continue;
    }
    int failure = win_entry_path(joined, sizeof joined, path, found);
    char *text = failure == 0 ? osp_host_string_copy(joined) : NULL;
    if (text == NULL) {
      return failure != 0 ? failure : ENOMEM;
    }
    osprey_list_builder_push_of(builder, (int64_t)(intptr_t)text, 1);
  } while (FindNextFileA(search, found));
  DWORD error = GetLastError();
  return error == ERROR_NO_MORE_FILES ? 0 : win_errno(error);
}

// Opens the `path\*` search, or INVALID_HANDLE_VALUE with *err set: a path
// that is not a directory is ENOTDIR, checked before any search starts.
static HANDLE win_open_search(const char *path, WIN32_FIND_DATAA *found, int *err) {
  char pattern[OSP_PATH_MAX];
  DWORD attributes = win_attributes(path, err);
  if (*err != 0 || (attributes & FILE_ATTRIBUTE_DIRECTORY) == 0) {
    *err = *err != 0 ? *err : ENOTDIR;
    return INVALID_HANDLE_VALUE;
  }
  if (osp_path_child(pattern, sizeof pattern, path, "*") == 0) {
    *err = ENAMETOOLONG;
    return INVALID_HANDLE_VALUE;
  }
  HANDLE search = FindFirstFileA(pattern, found);
  *err = search == INVALID_HANDLE_VALUE ? win_errno(GetLastError()) : 0;
  return search;
}

OspreyList *osprey_list_dir(const char *path) {
  osp_io_error_clear();
  if (path == NULL) {
    osp_io_error_text(OP_LIST_DIR, NULL, "no path");
    return NULL;
  }
  WIN32_FIND_DATAA found;
  int failure = 0;
  HANDLE search = win_open_search(path, &found, &failure);
  if (search == INVALID_HANDLE_VALUE) {
    osp_io_error_set(OP_LIST_DIR, path, failure);
    return NULL;
  }
  OspreyListBuilder *builder = osprey_list_builder_new_of(1);
  failure = builder == NULL ? ENOMEM : win_push_entries(search, &found, path, builder);
  (void)FindClose(search);
  return list_dir_sealed(builder, failure, path);
}

char *osprey_kind_of(const char *path) {
  osp_io_error_clear();
  int failure = path == NULL ? EINVAL : 0;
  DWORD attributes = path == NULL ? INVALID_FILE_ATTRIBUTES : win_attributes(path, &failure);
  if (failure != 0) {
    osp_io_error_set(OP_KIND_OF, path, failure);
    return NULL;
  }
  const char *kind = (attributes & FILE_ATTRIBUTE_DIRECTORY) != 0 ? "dir"
                     : (attributes & FILE_ATTRIBUTE_DEVICE) != 0  ? "other"
                                                                  : "file";
  char *copy = osp_host_string_copy(kind);
  if (copy == NULL) {
    osp_io_error_set(OP_KIND_OF, path, ENOMEM);
  }
  return copy;
}

int64_t osprey_make_dir(const char *path) {
  osp_io_error_clear();
  if (path == NULL) {
    osp_io_error_set(OP_MAKE_DIR, NULL, EINVAL);
    return -1;
  }
  if (path[0] != '\0' && CreateDirectoryA(path, NULL)) {
    return 0;
  }
  int failure = path[0] == '\0' ? ENOENT : win_errno(GetLastError());
  int existing = 0;
  DWORD attributes = failure == EEXIST ? win_attributes(path, &existing) : INVALID_FILE_ATTRIBUTES;
  if (failure == EEXIST && existing == 0 && (attributes & FILE_ATTRIBUTE_DIRECTORY) != 0) {
    return 0;
  }
  osp_io_error_set(OP_MAKE_DIR, path, failure == EEXIST ? ENOTDIR : failure);
  return -1;
}

#endif // OSPREY_FILE_DIRS_WIN_H
