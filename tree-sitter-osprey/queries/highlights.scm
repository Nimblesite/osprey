; Syntax highlighting for Osprey — replaces the TextMate (vscode-extension) and
; Monaco/Monarch (website playground) grammars with one canonical source.
; Capture names follow the tree-sitter highlight convention.

; --- Comments ---
(line_comment) @comment
(doc_comment) @comment.documentation

; --- Literals ---
(integer) @number
(float) @number
(boolean) @constant.builtin
(string) @string
(interpolated_string) @string

; --- Keywords ---
[
  "let" "mut" "fn" "extern" "type" "module" "import"
  "effect" "perform" "handle" "in"
  "match" "select" "where" "if" "else"
] @keyword

[
  "spawn" "yield" "await" "send" "recv"
] @keyword.control

; --- Operators ---
[
  "=" "->" "=>" "|>" "?" ":"
  "==" "!=" "<" ">" "<=" ">=" "&&" "||" "!" "%"
  "+" "-" "*" "/"
] @operator

; --- Punctuation ---
[ "(" ")" "{" "}" "[" "]" ] @punctuation.bracket
[ "," "." "|" ] @punctuation.delimiter

; --- Declarations & names ---
(function_declaration name: (identifier) @function)
(extern_declaration name: (identifier) @function)
(parameter name: (identifier) @variable.parameter)
(extern_parameter name: (identifier) @variable.parameter)
(let_declaration name: (identifier) @variable)
(effect_declaration name: (identifier) @type)
(operation_declaration name: (identifier) @function.method)

; --- Types ---
(type_declaration name: (identifier) @type)
(type_identifier (identifier) @type)
(generic_type name: (identifier) @type)
(array_type name: (identifier) @type)
(variant name: (identifier) @constructor)
(type_constructor name: (identifier) @type)
(type_parameter name: (identifier) @type.parameter)

; --- Effects ---
(effect_ref name: (identifier) @type)
(perform_expression effect: (identifier) @type operation: (identifier) @function.method)
(handler_expression effect: (identifier) @type)
(handler_arm operation: (identifier) @function.method)

; --- Calls & fields ---
(call_expression (identifier) @function.call)
(named_argument name: (identifier) @variable.parameter)
(field_assignment name: (identifier) @property)
(field_declaration name: (identifier) @property)

; --- Patterns ---
(pattern name: (identifier) @constructor)
"_" @variable.builtin

; --- Variable references (fallback) ---
(primary_expression (identifier) @variable)
