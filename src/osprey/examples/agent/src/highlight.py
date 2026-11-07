# osprey: flavor=python
# osprey: namespace=agent
# osprey: module=Highlight
"""Syntax colouring for the transcript: one language's code read into tokens,
each a stretch of the exact text and its kind, so every host draws the same
colours. One forward pass over the bytes, so a whole block costs one scan, and
joining every token's text gives the code back byte for byte. Strings and
comments win over everything inside them; what a language cannot say (nested
languages, JavaScript's regex literals) stays plain rather than guessed.
Pure, and tested without a window.
"""
__all__ = ["Token", "tokens_of", "language_of_path", "MAX_BYTES"]

MAX_BYTES = 200000
"""The most bytes of code that are scanned into tokens; a longer block draws as
one plain stretch, because the transcript is rebuilt on every change. A
2,000-line file is well under this and scans in milliseconds."""


class Token:
    """One stretch of code and its kind: `""` plain, or `keyword`, `string`, `comment`, `number`, `function`, `type`, `builtin`, `meta`, `inserted` or `removed`."""
    tokenText: str
    tokenKind: str


class Lang:
    """One language as a table, not code: its names and file extensions, its words, how its comments and strings are marked, and how its lines are read."""
    langNames: list[str]
    langExtensions: list[str]
    langKeywords: list[str]
    langTypes: list[str]
    langBuiltins: list[str]
    langDefs: list[str]
    langTypeDefs: list[str]
    langLine: list[str]
    langOpen: str
    langClose: str
    langStrings: list[str]
    langMultiline: list[str]
    langStyle: str
    langCaps: bool
    langFold: bool
    langLifetimes: bool
    langVars: bool
    langAttr: bool
    langHash: bool
    langHashStart: bool


def words_of(text):
    return unwrap_or(split(text, " "), [])


plain_lang = Lang(langNames=[], langExtensions=[], langKeywords=[], langTypes=[], langBuiltins=[], langDefs=[], langTypeDefs=[], langLine=[], langOpen="", langClose="", langStrings=["\"", "'"], langMultiline=[], langStyle="code", langCaps=True, langFold=False, langLifetimes=False, langVars=False, langAttr=False, langHash=False, langHashStart=False)


def block_commented(lang, line):
    """`lang` with C-style block comments and `line` as its line comment."""
    marked = update(lang, langOpen="/*", langClose="*/")
    return update(marked, langLine=line)


def c_words():
    """The words every C-family language shares."""
    return words_of("if else for while do switch case default break continue return goto struct union enum typedef")


def class_words():
    """The words every class-based language shares."""
    return words_of("public private protected static const virtual override abstract final")


def shared_words():
    """The class words and the C words the class-based languages share."""
    return listConcat(class_words(), c_words())


def own_words(own, shared):
    """A language's own words with its shared words after them."""
    return listConcat(words_of(own), shared)


CURLY_LINE = ["//"]
"""The line comment the C-family languages write."""


def word_lang(names, extensions, keywords, types, builtins, defs, typedefs):
    """A code language over the blank table: its names, extensions, words and definition openers."""
    return update(plain_lang, langNames=names, langExtensions=extensions, langKeywords=keywords, langTypes=types, langBuiltins=builtins, langDefs=defs, langTypeDefs=typedefs)


def triple_attr_lang(names, extensions, keywords, types, builtins, defs, typedefs):
    """A class-based language with triple-quoted strings and `@` attributes."""
    quoted = update(word_lang(names, extensions, keywords, types, builtins, defs, typedefs), langStrings=["\"\"\"", "\"", "'"], langMultiline=["\"\"\""], langAttr=True)
    return block_commented(quoted, CURLY_LINE)


def literal_lang(names, extensions, builtins, line, style):
    """A data language over the blank table: its names, extensions, literals, line comment and style."""
    return update(plain_lang, langNames=names, langExtensions=extensions, langBuiltins=builtins, langLine=line, langStyle=style, langCaps=False)


def build_tables():
    """The language tables in scan order: aliases map onto the first match."""
    python_lang = update(word_lang(["python", "py"], ["py", "pyi"], words_of("if else for while return break continue def class lambda import from as global nonlocal pass raise del with yield assert async await elif and or not in is try except finally match case"), words_of("int float str bool list dict set tuple bytes bytearray frozenset complex object type"), words_of("print len range enumerate zip map filter sorted sum min max abs any all open input isinstance super self None True False repr round format divmod next"), ["def", "lambda"], ["class"]), langLine=["#"], langStrings=["\"\"\"", "'''", "\"", "'"], langMultiline=["\"\"\"", "'''"], langAttr=True)
    javascript_lang = block_commented(update(word_lang(["javascript", "js", "jsx"], ["js", "jsx", "mjs", "cjs"], own_words("var let const function class extends super new typeof instanceof in of delete void throw yield async await try catch finally export import default get set", c_words()), words_of("string number boolean any unknown never bigint symbol object"), words_of("console Math JSON Object Array String Number Boolean Promise Map Set Date RegExp Error window document process require module exports undefined NaN Infinity this null true false"), ["function"], ["class", "interface", "enum"]), langStrings=["\"", "'", "`"], langMultiline=["`"], langAttr=True), CURLY_LINE)
    typescript_lang = update(javascript_lang, langNames=["typescript", "ts", "tsx"], langExtensions=["ts", "tsx", "mts", "cts"], langKeywords=listConcat(javascript_lang.langKeywords, words_of("type namespace interface implements readonly declare abstract satisfies keyof infer as is enum")), langTypes=listConcat(javascript_lang.langTypes, words_of("unknown never void")))
    rust_lang = block_commented(update(word_lang(["rust", "rs"], ["rs"], own_words("fn let mut match loop impl pub mod use crate super as dyn ref move async await unsafe extern where", c_words()), words_of("i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 usize isize f32 f64 bool char str String Vec Option Result Box Rc Arc HashMap BTreeMap Self"), words_of("println print vec format write Some None Ok Err self assert stringify"), ["fn"], ["struct", "enum", "trait", "type", "union"]), langLifetimes=True, langHash=True), CURLY_LINE)
    go_lang = block_commented(update(word_lang(["go", "golang"], ["go"], own_words("func var const type interface map chan go defer range fallthrough select package nil", c_words()), words_of("int int8 int16 int32 int64 uint uint8 uint16 uint32 uint64 uintptr string bool byte rune float32 float64 complex64 complex128 error any"), words_of("append make new len cap copy delete panic recover print println close self nil"), ["func"], ["struct", "interface", "type"]), langStrings=["\"", "'", "`"], langMultiline=["`"]), CURLY_LINE)
    swift_lang = triple_attr_lang(["swift"], ["swift"], own_words("func class protocol extension guard repeat defer throws try catch where init deinit subscript required convenience lazy weak unowned mutating nonmutating override open internal self super nil", shared_words()), words_of("Int Double Float Bool String Character Array Dictionary Set Optional Any AnyObject Self"), words_of("print self super nil true false"), ["func", "init"], ["class", "struct", "enum", "protocol", "typealias", "extension", "actor"])
    c_lang = block_commented(update(word_lang(["c"], ["c"], c_words(), words_of("int char float double void long short unsigned signed size_t ssize_t bool FILE uint8_t uint32_t uint64_t int32_t int64_t"), words_of("printf scanf puts malloc free memcpy memset strlen fgets fopen fclose NULL self this"), [], ["struct", "union", "enum", "typedef"]), langHash=True), CURLY_LINE)
    cpp_lang = update(c_lang, langNames=["cpp", "c++", "h", "hpp"], langExtensions=["cpp", "cc", "cxx", "hpp", "hh", "h", "ipp", "tpp"], langKeywords=own_words("class template namespace using new delete this operator friend constexpr constinit consteval nullptr concept requires co_await co_return", listConcat(c_words(), class_words())), langTypes=listConcat(c_lang.langTypes, words_of("auto std string vector map set unordered_map array span optional variant tuple")), langBuiltins=listConcat(c_lang.langBuiltins, words_of("cout cin endl std nullptr this true false move forward")), langTypeDefs=["class", "struct", "enum", "union", "typedef", "namespace", "using"])
    java_lang = block_commented(update(word_lang(["java"], ["java"], own_words("class package import new this super instanceof throws synchronized volatile native transient strictfp var record sealed permits yield", shared_words()), words_of("int long short byte char float double boolean void String Object Integer Long Double Boolean Character List Map Set ArrayList HashMap HashSet Optional Stream"), words_of("System out err println print printf Math self this null true false"), [], ["class", "interface", "enum", "record"]), langAttr=True), CURLY_LINE)
    kotlin_lang = triple_attr_lang(["kotlin", "kt"], ["kt", "kts"], own_words("class fun val var object data sealed when by lazy lateinit override open companion init constructor this super try catch finally throw import package as is in null it", shared_words()), words_of("Int Long Short Byte Char Float Double Boolean String List Map Set Array MutableList MutableMap Unit Any Nothing Sequence"), words_of("println print self this null true false listOf mapOf setOf mutableListOf"), ["fun"], ["class", "object", "interface", "enum", "typealias"])
    csharp_lang = block_commented(word_lang(["csharp", "cs", "c#"], ["cs"], own_words("class using namespace var new lock ref out in is as get set value this base nameof typeof unchecked checked unsafe fixed await async yield record readonly", shared_words()), words_of("int long short byte char float double bool decimal string object dynamic void nint nuint String Int32 Int64 List Dictionary HashSet Task Action Func IEnumerable"), words_of("Console WriteLine Write ReadLine ReadKey Math self this null true false"), [], ["class", "struct", "interface", "enum", "record", "namespace"]), CURLY_LINE)
    ruby_lang = update(word_lang(["ruby", "rb"], ["rb"], words_of("def end class module if elsif else unless case when while until for then begin rescue ensure raise return yield break next super self nil true false and or not require require_relative attr_accessor attr_reader attr_writer puts lambda proc do"), words_of("Integer Float String Symbol Array Hash Struct Set Regexp Range"), words_of("puts print p require self nil true false lambda proc new"), ["def"], ["class", "module", "struct"]), langLine=["#"])
    php_lang = block_commented(update(word_lang(["php"], ["php"], own_words("class function echo print use namespace instanceof global yield match enum fn readonly list foreach elseif unset isset clone", shared_words()), words_of("int float string bool array object callable void mixed static"), words_of("echo print isset unset count strlen array_map array_filter self this null true false"), ["function", "fn"], ["class", "interface", "trait", "enum"]), langVars=True), ["//", "#"])
    bash_lang = update(plain_lang, langNames=["bash", "sh", "shell", "zsh", "console"], langExtensions=["sh", "bash", "zsh"], langKeywords=words_of("if then else elif fi for while until do done case esac function return break continue local export readonly declare unset shift exit source alias in select coproc time set trap"), langBuiltins=words_of("echo printf read cd pwd ls cat grep sed awk cut sort uniq head tail xargs self this true false"), langLine=["#"], langCaps=False, langVars=True, langHashStart=True)
    sql_lang = block_commented(update(word_lang(["sql"], ["sql"], words_of("select from where group by order having limit offset insert into values update set delete create table drop alter add index view join inner left right outer full cross on as and or not in between like is distinct union all case when then else end with recursive primary key references constraint default cascade using"), words_of("int integer bigint smallint decimal numeric float real double char varchar text date time timestamp timestamptz boolean blob bytea serial uuid json jsonb xml"), words_of("count sum avg min max coalesce nullif now current_timestamp current_date cast exists null true false self"), [], []), langCaps=False, langFold=True), ["--"])
    json_lang = update(literal_lang(["json"], ["json"], ["true", "false", "null"], ["//"], "json"), langStrings=["\""])
    yaml_lang = literal_lang(["yaml", "yml"], ["yaml", "yml"], ["true", "false", "null", "yes", "no", "nil", "~"], ["#"], "yaml")
    toml_lang = literal_lang(["toml"], ["toml"], ["true", "false", "inf", "nan"], ["#"], "toml")
    css_lang = block_commented(update(word_lang(["css", "scss"], ["css", "scss"], words_of("media supports import keyframes font-face charset else only not and from to layer container"), words_of("hover focus active visited link checked disabled enabled before after first-child last-child first-of-type root empty target"), words_of("important inherit initial unset auto none inline block flex grid inline-flex absolute relative fixed sticky hidden visible static"), [], []), langStyle="css", langCaps=False), [])
    html_lang = update(plain_lang, langNames=["html", "xml", "svg"], langExtensions=["html", "htm", "xml", "svg", "xhtml"], langOpen="<!--", langClose="-->", langStyle="html", langCaps=False)
    markdown_lang = update(plain_lang, langNames=["markdown", "md"], langExtensions=["md", "markdown", "mdx"], langStrings=["`"], langOpen="<!--", langClose="-->", langStyle="markdown", langCaps=False)
    diff_lang = update(plain_lang, langNames=["diff", "patch"], langExtensions=["diff", "patch"], langStrings=[], langStyle="diff", langCaps=False)
    osprey_lang = block_commented(update(word_lang(["osprey", "ospml", "osp"], ["osp", "ospml"], words_of("fn let match module export import type effect perform handle handler extern namespace if else true false mut spawn await yield send recv control resume as in return finally static where test expect def class lambda case from with try except while for break continue pass raise assert del"), words_of("List Map Result Option Signal String Int Float Bool Char Byte Fiber Effect Tensor"), words_of("print toString length byteLength range mapList filterList foldList forEachList listAppend listConcat listGet unwrap_or satAdd satSub contains startsWith endsWith indexOf take drop split join trim"), ["fn", "def"], ["type", "class", "module", "enum", "effect", "extern", "namespace"]), langStrings=["\""]), ["//", "#"])
    return [python_lang, javascript_lang, typescript_lang, rust_lang, go_lang, swift_lang, c_lang, cpp_lang, java_lang, kotlin_lang, csharp_lang, ruby_lang, php_lang, bash_lang, sql_lang, json_lang, yaml_lang, toml_lang, css_lang, html_lang, markdown_lang, diff_lang, osprey_lang]


def table_of(language):
    """The table `language` names, matched case-insensitively with its aliases once, or a table with no names."""
    name = toLowerCase(trim(language))
    found = filterList(build_tables(), lambda lang: listContains(lang.langNames, name))
    return unwrap_or(listGet(found, 0), plain_lang)


def extend(acc, lang):
    name = unwrap_or(listGet(lang.langNames, 0), "")
    return foldList(lang.langExtensions, acc, lambda m, ext: mapSet(m, toLowerCase(ext), name))


def extension_map():
    return foldList(build_tables(), {}, extend)


def from_end(text, mark):
    """Where `mark` last occurs in `text`, -1 when never: the end is measured through a reversal."""
    match indexOf(reverse(text), mark):
        case Success(value=at):
            return byteLength(text) - 1 - at
        case Error(message=_m):
            return -1


def language_of_path(path):
    """The language a file path's extension names, "" when it names none: a dotfile has no extension."""
    name = drop(path, from_end(path, "/") + 1)
    at = from_end(name, ".")
    ext = toLowerCase(drop(name, at + 1))
    return "" if at <= 0 else unwrap_or(mapGet(extension_map(), ext), "")


# The scan: one forward pass over the code's lines. The code is split into
# lines once, so every byte and substring operation runs on a short line, not
# the whole block ([BUILTIN-STRING-CURSOR]); an open multi-line string or block
# comment carries across lines in `scanBuf`.

class Scan:
    """The pass so far: its tokens, the language, the open span's mode, mark and buffer, and the per-line fields the byte scanner carries."""
    scanTokens: list[Token]
    scanLang: Lang
    scanMode: str
    scanMark: str
    scanBack: str
    scanBuf: str
    scanCode: str
    scanStart: int
    scanSkip: int
    scanRun: str
    scanRunAt: int
    scanPrev: str
    scanEsc: bool
    scanLineStart: bool


def tokens_of(language, code):
    """The tokens of `code` written in `language`: joining every token's text
    gives `code` back exactly. An unknown language tries nothing, and code past
    `MAX_BYTES` draws as one plain stretch."""
    lang = table_of(language)
    return [Token(tokenText=code, tokenKind="")] if length(lang.langNames) == 0 or byteLength(code) > MAX_BYTES else scan_code(lang, code)


def scan_code(lang, code):
    """Scans `code` under a known language, line by line."""
    lines = unwrap_or(split(code, "\n"), [code])
    start = Scan(scanTokens=[], scanLang=lang, scanMode="", scanMark="", scanBack="", scanBuf="", scanCode="", scanStart=0, scanSkip=0, scanRun="", scanRunAt=-1, scanPrev="", scanEsc=False, scanLineStart=True)
    return close_all(fold(range(0, len(lines)), start, lambda acc, i: scan_line(acc, i, lines)))


def scan_line(acc, i, lines):
    """Reads one line: an open multi-line span consumes it, otherwise the byte scanner runs."""
    line = unwrap_or(listGet(lines, i), "")
    last = i == len(lines) - 1
    if acc.scanMode == "str":
        return span_line(acc, line, last, acc.scanMark, lambda text: update(acc, scanTokens=emit(acc, text, "string"), scanBuf="", scanMark="", scanMode=acc.scanBack, scanBack=""))
    elif acc.scanMode == "block":
        return span_line(acc, line, last, acc.scanLang.langClose, lambda text: update(acc, scanTokens=emit(acc, text, "comment"), scanBuf="", scanMode=""))
    else:
        return single_line(acc, line, last, True)


def span_line(acc, line, last, mark, close):
    """Consume an open multiline span and resume scanning after its closing delimiter."""
    match indexOf(line, mark):
        case Success(value=at):
            end = at + byteLength(mark)
            base = close(acc.scanBuf + take(line, end))
            return single_line(base, drop(line, end), last, False)
        case Error(message=_m):
            return update(acc, scanBuf=acc.scanBuf + line + ("" if last else "\n"))


def single_line(acc, line, last, fresh):
    """One line's byte scan in normal mode, then its end-of-line closing."""
    base = update(acc, scanCode=line, scanStart=0, scanSkip=0, scanRun="", scanRunAt=-1, scanPrev="" if fresh else acc.scanPrev, scanEsc=False, scanLineStart=fresh)
    scanned = fold(range(0, byteLength(line)), base, scan_step)
    return close_line(scanned, line, last)


def carry_buf(acc, line, last, mode):
    """A span open at a line's end: its tail joins the buffer and the span carries past the newline."""
    return update(acc, scanBuf=acc.scanBuf + text_of(line, acc.scanStart, byteLength(line)) + ("" if last else "\n"), scanMode=mode)


def emit_tail(acc, line, kind, mode):
    """A span the line's end closes: its tail emits whole as one token and the span ends."""
    return update(acc, scanTokens=emit(acc, text_of(line, acc.scanStart, byteLength(line)), kind), scanMode=mode, scanBack="")


def close_line(acc, line, last):
    """Closes whatever a line left open, then the line's newline."""
    if acc.scanMode == "str":
        return close_str(acc, line, last)
    elif acc.scanMode == "block":
        return carry_buf(acc, line, last, "block")
    elif acc.scanMode == "line":
        return append_newline(emit_tail(acc, line, "comment", ""), last)
    elif acc.scanMode == "whole" or acc.scanMode == "tagname":
        return append_newline(emit_tail(acc, line, acc.scanMark, ""), last)
    elif acc.scanMode == "bracket":
        return append_newline(emit_tail(acc, line, "meta", ""), last)
    elif acc.scanMode == "tag":
        return append_newline(update(acc, scanMode=""), last)
    else:
        closed = close_run(acc, byteLength(line), 0) if acc.scanRun != "" or acc.scanRunAt >= 0 else acc
        return append_newline(closed, last)


def close_str(acc, line, last):
    """A string open at a line's end: a multi-line one carries on, a single-line one ends."""
    if listContains(acc.scanLang.langMultiline, acc.scanMark):
        return carry_buf(acc, line, last, "str")
    else:
        return append_newline(emit_tail(acc, line, "string", ""), last)


def append_newline(acc, last):
    """The newline after a line, as a plain token, except past the last."""
    return update(acc, scanTokens=emit(acc, "\n", "")) if not last else acc


def flush_buf(acc, kind):
    """A span open at the code's end emits its whole buffer as one token."""
    return update(acc, scanTokens=emit(acc, acc.scanBuf, kind)).scanTokens


def close_all(acc):
    """Closes a multi-line span still open at the end of the code."""
    if acc.scanMode == "str":
        return flush_buf(acc, "string")
    elif acc.scanMode == "block":
        return flush_buf(acc, "comment")
    else:
        return acc.scanTokens


def byte_at(code, i):
    return unwrap_or(byteAt(code, i), 0)


def text_of(code, from_at, to):
    return unwrap_or(substring(code, from_at, least(to, byteLength(code))), "")


def least(a, b):
    return a if a < b else b


def hit(code, i, mark):
    """Whether `mark` is written at `i`."""
    return unwrap_or(substring(code, i, least(i + byteLength(mark), byteLength(code))), "") == mark


def is_upper(b):
    return b >= 65 and b <= 90


def is_digit(b):
    return b >= 48 and b <= 57


def is_word_start(b):
    return (b >= 65 and b <= 90) or (b >= 97 and b <= 122) or b == 95


def is_word_byte(b):
    return is_word_start(b) or is_digit(b) or b >= 128


def is_hex(b):
    return is_digit(b) or (b >= 65 and b <= 70) or (b >= 97 and b <= 102)


def emit(acc, text, kind):
    return listAppend(acc.scanTokens, Token(tokenText=text, tokenKind=kind))


def ahead(code, i, n):
    """The first byte at or after `i` that is neither a space nor a tab, 0 past the end or a long run."""
    b = byte_at(code, i)
    return 0 if i >= byteLength(code) or n > 4096 else (ahead(code, i + 1, n + 1) if b == 32 or b == 9 else b)


def scan_step(acc, i):
    """Reads one byte of a line in whatever is open."""
    b = byte_at(acc.scanCode, i)
    return acc if i < acc.scanSkip else step_dispatch(acc, i, b)


def step_dispatch(acc, i, b):
    match acc.scanMode:
        case "str":
            return str_step(acc, i, b)
        case "block":
            return block_step(acc, i)
        case "whole":
            return acc
        case "line":
            return acc
        case "tagname":
            return tagname_step(acc, i, b)
        case "tag":
            return tag_step(acc, i, b)
        case "bracket":
            return bracket_step(acc, i, b)
        case _:
            return plain_step(acc, i, b)


def plain_step(acc, i, b):
    """Reads one plain byte: the line rules at a line's start, then the open run, then what starts here."""
    ruled = line_rule(acc, i) if acc.scanLineStart else acc
    if ruled.scanMode != "" or ruled.scanSkip > i:
        return update(ruled, scanLineStart=False)
    else:
        return run_or_start(update(ruled, scanLineStart=False), i, b)


def run_or_start(acc, i, b):
    """Continues the open run, or closes it and starts whatever the byte opens."""
    if acc.scanRun != "" or acc.scanRunAt >= 0:
        if run_continues(acc, i, b):
            return acc
        else:
            return start_at(close_run(acc, i, b), i, b)
    else:
        return start_at(acc, i, b)


def start_at(acc, i, b):
    """What the byte at `i` opens: a lifetime, a comment, a string, a tag, a bracket, or a run."""
    lang = acc.scanLang
    code = acc.scanCode
    mark = string_at(acc, i)
    run = run_at(acc, i, b)
    if b == 39 and lang.langLifetimes and lifetime_at(code, i):
        return update(acc, scanRun="life", scanRunAt=i)
    elif lang.langOpen != "" and hit(code, i, lang.langOpen):
        return update(acc, scanMode="block", scanStart=i)
    elif line_comment_at(acc, i):
        return update(acc, scanMode="line", scanStart=i)
    elif mark != "":
        return update(acc, scanMode="str", scanMark=mark, scanBack="", scanStart=i, scanSkip=i + byteLength(mark))
    elif lang.langStyle == "html" and b == 60:
        return html_at(acc, i)
    elif lang.langHash and hit(code, i, "#["):
        return update(acc, scanMode="bracket", scanStart=i, scanSkip=i + 2)
    elif run != "":
        return update(acc, scanRun=run, scanRunAt=i)
    else:
        return update(acc, scanRunAt=i)


def line_comment_at(acc, i):
    """Whether a line comment starts at `i`; bash's `#` only at a word's start."""
    lang = acc.scanLang
    code = acc.scanCode
    marked = filterList(lang.langLine, lambda m: hit(code, i, m))
    return len(marked) > 0 and (not lang.langHashStart or at_word_start(code, i))


def at_word_start(code, i):
    return i == 0 or contains("\n \t;&|(", unwrap_or(substring(code, i - 1, i), "x"))


def string_at(acc, i):
    """The string delimiter written at `i`, longest first, "" when none."""
    found = filterList(acc.scanLang.langStrings, lambda d: hit(acc.scanCode, i, d))
    return unwrap_or(listGet(found, 0), "")


def lifetime_at(code, i):
    """Whether `'` at `i` opens a lifetime, not a character literal."""
    return is_word_start(byte_at(code, i + 1)) and byte_at(code, i + 1) != 92 and byte_at(code, i + 2) != 39


def run_at(acc, i, b):
    """The run the byte at `i` opens: a word, a number, a variable, an attribute, or a CSS selector."""
    lang = acc.scanLang
    code = acc.scanCode
    if is_word_start(b):
        return "word"
    elif is_digit(b):
        return "num"
    elif b == 36 and lang.langVars and (is_word_start(byte_at(code, i + 1)) or byte_at(code, i + 1) == 123):
        return "var"
    elif b == 64 and lang.langAttr and is_word_start(byte_at(code, i + 1)):
        return "attr"
    elif b == 35 and lang.langHash and is_word_start(byte_at(code, i + 1)):
        return "attr"
    elif lang.langStyle == "css":
        return css_run_at(code, i, b)
    else:
        return ""


def css_run_at(code, i, b):
    """The run a CSS byte opens: a hex colour, a class or id selector, a pseudo-class or `!important`."""
    n = byte_at(code, i + 1)
    if b == 35:
        return "hex" if listContains([3, 4, 6, 8], hex_len(code, i + 1, 0)) else ("id" if is_word_start(n) else "")
    elif b == 46 and is_word_start(n):
        return "id"
    elif b == 58 and (is_word_start(n) or n == 58):
        return "pseudo"
    elif b == 33 and is_word_start(n):
        return "bang"
    else:
        return ""


def hex_len(code, i, n):
    return hex_len(code, i + 1, n + 1) if n < 9 and is_hex(byte_at(code, i)) else n


def run_continues(acc, i, b):
    """Whether the open run swallows the byte at `i`."""
    code = acc.scanCode
    match acc.scanRun:
        case "":
            return not plain_interesting(acc, i, b)
        case "word":
            return is_word_byte(b)
        case "num":
            return is_word_byte(b) or b == 37 or ((b == 43 or b == 45) and (byte_at(code, i - 1) == 101 or byte_at(code, i - 1) == 69)) or (b == 46 and is_digit(byte_at(code, i + 1)))
        case "var":
            return is_word_byte(b) or b == 123 or b == 125
        case "life":
            return is_word_byte(b)
        case "attr":
            return is_word_byte(b)
        case "hex":
            return is_hex(b)
        case "id":
            return is_word_byte(b)
        case "pseudo":
            return is_word_byte(b) or b == 58
        case _:
            return is_word_byte(b)


def plain_interesting(acc, i, b):
    """Whether the byte at `i` can start anything but more plain text."""
    lang = acc.scanLang
    code = acc.scanCode
    if is_word_start(b) or is_digit(b):
        return True
    elif b == 34 or b == 39 or b == 96:
        return True
    elif b == 47:
        return byte_at(code, i + 1) == 47 or byte_at(code, i + 1) == 42
    elif b == 45:
        return byte_at(code, i + 1) == 45
    elif b == 35:
        return lang.langHash or lang.langStyle == "css" or listContains(lang.langLine, "#")
    elif b == 64:
        return lang.langAttr
    elif b == 36:
        return lang.langVars
    elif b == 60:
        return lang.langStyle == "html"
    elif lang.langStyle == "css" and (b == 46 or b == 58 or b == 33):
        return True
    else:
        return False


def word_kind(acc, i, b, word):
    """One whole word's kind: what it is named by, its table, or what follows it."""
    lang = acc.scanLang
    name = toLowerCase(word) if lang.langFold else word
    if listContains(lang.langDefs, acc.scanPrev):
        return "function"
    elif listContains(lang.langTypeDefs, acc.scanPrev):
        return "type"
    elif listContains(lang.langKeywords, name):
        return "keyword"
    elif listContains(lang.langTypes, name):
        return "type"
    elif b == 40:
        return "function"
    elif listContains(lang.langBuiltins, name):
        return "builtin"
    elif lang.langCaps and is_upper(byte_at(word, 0)):
        return "type"
    elif lang.langStyle == "css" and ahead(acc.scanCode, i, 0) == 123:
        return "keyword"
    elif lang.langStyle == "css" and b == 58:
        return "meta"
    else:
        return ""


def run_kind(acc, i, b):
    """The token kind the open run closes as."""
    match acc.scanRun:
        case "word":
            return word_kind(acc, i, b, text_of(acc.scanCode, acc.scanRunAt, i))
        case "num":
            return "number"
        case "var":
            return "meta"
        case "life":
            return "type"
        case "attr":
            return "meta"
        case "hex":
            return "number"
        case "id":
            return "keyword"
        case "pseudo":
            return "type"
        case "bang":
            return "keyword"
        case _:
            return ""


def close_run(acc, i, b):
    """Emits the open run whole and clears it, remembering the word."""
    text = text_of(acc.scanCode, acc.scanRunAt, i)
    word = acc.scanRun == "word"
    return update(acc, scanTokens=emit(acc, text, run_kind(acc, i, b)), scanRun="", scanRunAt=-1, scanPrev=text if word else acc.scanPrev)


def str_step(acc, i, b):
    """Reads one byte of an open string: an escape, or the closing delimiter."""
    code = acc.scanCode
    if acc.scanEsc:
        return update(acc, scanEsc=False)
    elif b == 92:
        return update(acc, scanEsc=True)
    elif hit(code, i, acc.scanMark):
        end = i + byteLength(acc.scanMark)
        return update(acc, scanTokens=emit(acc, text_of(code, acc.scanStart, end), key_kind(acc, end)), scanMode=acc.scanBack, scanBack="", scanSkip=end)
    else:
        return acc


def key_kind(acc, end):
    """A closed string's kind: a JSON or quoted YAML key reads as meta, by the colon behind it."""
    style = acc.scanLang.langStyle
    return "meta" if acc.scanBack == "" and (style == "json" or style == "yaml") and ahead(acc.scanCode, end, 0) == 58 else "string"


def block_step(acc, i):
    """Reads one byte of a block comment, to its closing mark."""
    code = acc.scanCode
    end = i + byteLength(acc.scanLang.langClose)
    return update(acc, scanTokens=emit(acc, text_of(code, acc.scanStart, end), "comment"), scanMode="", scanSkip=end) if hit(code, i, acc.scanLang.langClose) else acc


def bracket_step(acc, i, b):
    """Reads one byte of a bracketed attribute, to its closing bracket."""
    return update(acc, scanTokens=emit(acc, text_of(acc.scanCode, acc.scanStart, i + 1), "meta"), scanMode="", scanSkip=i + 1) if b == 93 else acc


def html_at(acc, i):
    """`<` opening a tag: its name is read next, `<!` a doctype, anything else stays plain."""
    code = acc.scanCode
    n = byte_at(code, i + 1)
    slash = n == 47
    lead = byte_at(code, i + 2) if slash else n
    if is_word_start(lead) or (not slash and n == 33):
        width = 2 if slash else 1
        base = update(acc, scanTokens=emit(acc, text_of(code, i, i + width), ""))
        kind = "meta" if n == 33 else "keyword"
        return update(base, scanMode="tagname", scanMark=kind, scanStart=i + width, scanSkip=i + width)
    else:
        return update(acc, scanRunAt=i)


def tagname_step(acc, i, b):
    """Reads one byte of a tag's name, then the attribute list the name opens."""
    code = acc.scanCode
    if is_word_byte(b) or b == 45 or b == 58:
        return acc
    else:
        closed = update(acc, scanTokens=emit(acc, text_of(code, acc.scanStart, i), acc.scanMark), scanMode="tag")
        return tag_step(closed, i, b)


def tag_step(acc, i, b) -> Scan:
    """Reads one byte inside a tag: attributes as meta, quoted values as strings, and the tag's end."""
    code = acc.scanCode
    if acc.scanRun == "attr":
        if is_word_byte(b):
            return acc
        else:
            return tag_step(close_run(acc, i, b), i, b)
    elif b == 62:
        closed = close_run(acc, i, b) if acc.scanRunAt >= 0 else acc
        return update(closed, scanMode="", scanRunAt=i)
    elif b == 34 or b == 39:
        base = close_run(acc, i, b) if acc.scanRunAt >= 0 else acc
        return update(base, scanMode="str", scanMark=text_of(code, i, i + 1), scanBack="tag", scanStart=i, scanSkip=i + 1)
    elif is_word_start(b):
        base = close_run(acc, i, b) if acc.scanRunAt >= 0 else acc
        return update(base, scanRun="attr", scanRunAt=i)
    else:
        return acc if acc.scanRunAt >= 0 else update(acc, scanRunAt=i)


def line_rule(acc, i):
    """A line's first byte in the line-classified styles: diff, markdown, and the keys of yaml and toml."""
    base = acc
    style = base.scanLang.langStyle
    if style == "diff":
        return diff_rule(base, i)
    elif style == "markdown":
        return markdown_rule(base, i)
    elif style == "yaml":
        return key_rule(base, i)
    elif style == "toml":
        return toml_rule(base, i)
    else:
        return acc


def whole_line(base, i, mark):
    """A line-classified line emits whole as one token of `mark`'s kind."""
    return update(base, scanMode="whole", scanMark=mark, scanStart=i)


def diff_rule(base, i):
    """A diff line whole: headers and hunks as meta, `+` inserted, `-` removed."""
    code = base.scanCode
    if hit(code, i, "+++") or hit(code, i, "---") or hit(code, i, "@@"):
        return whole_line(base, i, "meta")
    else:
        mark = "inserted" if byte_at(code, i) == 43 else ("removed" if byte_at(code, i) == 45 else "")
        return whole_line(base, i, mark) if mark != "" else base


def markdown_rule(base, i):
    """A markdown line: a heading, a quote, a fence, or an ordered item's number."""
    code = base.scanCode
    b = byte_at(code, i)
    end = marker_end(code, i, 0)
    if b == 35:
        return whole_line(base, i, "keyword")
    elif b == 62:
        return whole_line(base, i, "comment")
    elif hit(code, i, "```"):
        return whole_line(base, i, "meta")
    elif end > 0:
        return update(base, scanTokens=emit(base, text_of(code, i, end), "number"), scanSkip=end)
    else:
        return base


def marker_end(code, i, depth):
    """Where an ordered item's `1. ` marker ends, -1 when the line starts with no such marker."""
    b = byte_at(code, i)
    return marker_end(code, i + 1, depth + 1) if is_digit(b) and depth < 4 else (i + 1 if b == 46 else -1)


def key_rule(base, i):
    """A key line's name, from the line's start to its `:`, `=` or whitespace, as one meta token."""
    code = base.scanCode
    j = key_end(code, i, 0)
    if j <= 0 or line_comment_at(base, i):
        return base
    else:
        return update(base, scanTokens=emit(base, text_of(code, i, j), "meta"), scanSkip=j)


def toml_rule(base, i):
    """A toml line: a `[section]` as one type token, or a key line's name to its `=`."""
    code = base.scanCode
    j = key_scan(code, i, 93, 0)
    if byte_at(code, i) == 91 and j > 0:
        return update(base, scanTokens=emit(base, text_of(code, i, j + 1), "type"), scanSkip=j + 1)
    else:
        return key_rule(base, i)


def key_scan(code, j, stop, depth):
    """Where the `stop` byte first sits on the line, -1 at a quote, the line's end or 512 bytes."""
    b = byte_at(code, j)
    return -1 if depth > 512 or b == 10 or b == 0 or b == 34 or b == 39 else (j if b == stop else key_scan(code, j + 1, stop, depth + 1))


def key_end(code, j, depth):
    """Where a key's name ends: the first space, tab, colon or equals, -1 at the line's end or 512 bytes."""
    b = byte_at(code, j)
    return -1 if depth > 512 or b == 10 or b == 0 else (j if b == 32 or b == 9 or b == 58 or b == 61 else key_end(code, j + 1, depth + 1))
