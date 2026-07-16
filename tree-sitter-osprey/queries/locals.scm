; Scopes and bindings for editor "go to definition" / rename (lspkit/LSP).

(function_declaration) @local.scope
(lambda_expression) @local.scope
(block) @local.scope
(match_arm) @local.scope
(handler_expression) @local.scope
(module_declaration) @local.scope
(namespace_declaration) @local.scope
(signature_declaration) @local.scope

(let_declaration name: (identifier) @local.definition)
(parameter name: (identifier) @local.definition)
(extern_parameter name: (identifier) @local.definition)
(field_pattern (identifier) @local.definition)
(signature_value name: (identifier) @local.definition)

(primary_expression (identifier) @local.reference)
