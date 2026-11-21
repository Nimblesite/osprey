// [DAW-PROJECT-MODEL] Project the stored root and its transitive field types from typeDiagram.
export function projectCodec(model, root) {
  const types = new Map(model.decls.map(type => [type.name, type]))
  const output = []
  const emitted = new Set()
  const scalars = { String: 'JsonString', Int: 'JsonInt', Float: 'JsonFloat', Bool: 'JsonBool' }
  const readers = { String: 'text', Int: 'number', Float: 'decimal', Bool: 'flag' }
  const usedMembers = new Set()
  function reader(name, fields) {
    const args = []
    const values = fields.map(field => {
      const method = readers[field.type.name]
      if (!method) args.push(field.name)
      return field.name + ' = ' + (method ? `ui::Doc::${method} (doc, "${field.name}")` : field.name)
    })
    const usesDoc = fields.some(field => readers[field.type.name])
    if (usesDoc) args.unshift('doc')
    const params = args.length === 1 ? args[0] : '(' + args.join(', ') + ')'
    return `    (** Read the fields of ${name} after its nested values are decoded. *)\n    export read${name} ${params} = ${name}(${values.join(', ')})`
  }
  function writer(ref) {
    if (scalars[ref.name]) return scalars[ref.name]
    emit(ref.name)
    return 'write' + ref.name
  }
  function scalarMember(type, name, value) {
    usedMembers.add(type)
    return `${readers[type]}Member ("${name}", ${value})`
  }
  function member(field, variable) {
    const ref = field.type
    if (scalars[ref.name]) return scalarMember(ref.name, field.name, variable)
    const val = ref.name === 'List'
      ? `JsonArray (mapList (${variable}, ${writer(ref.args[0])}))`
      : `${writer(ref)} ${variable}`
    return `JsonMember "${field.name}" (${val})`
  }
  function variantLine(variant, enumeration) {
    const vars = variant.fields.map(field => field.name)
    const pattern = [variant.name, ...vars].join(' ')
    const tag = `"${variant.name}"`
    if (enumeration) return `        ${pattern} => JsonString ${tag}`
    const fields = variant.fields.map((field, i) => member(field, vars[i]))
    return `        ${pattern} => JsonObject [${[scalarMember('String', 'kind', tag), ...fields].join(', ')}]`
  }
  function emit(name) {
    if (emitted.has(name)) return
    emitted.add(name)
    const type = types.get(name)
    if (!type || !['record', 'union'].includes(type.kind)) throw new Error('Unsupported project type ' + name)
    const signature = (name === root ? '    (** Encode the complete project document. *)\n' : '') + '    ' + (name === root ? 'export ' : '') + 'write' + name + ' item = '
    if (type.kind === 'union') {
      const enumeration = type.variants.every(variant => variant.fields.length === 0)
      const lines = type.variants.map(variant => variantLine(variant, enumeration))
      output.push(signature + 'match item\n' + lines.join('\n'))
      for (const variant of type.variants.filter(variant => variant.fields.length)) output.push(reader(variant.name, variant.fields))
    } else {
      const fields = type.fields.map(field => member(field, 'item.' + field.name))
      output.push(signature + 'JsonObject [' + fields.join(', ') + ']')
      output.push(reader(name, type.fields))
    }
  }
  emit(root)
  const helpers = [...usedMembers].map(type => `    ${readers[type]}Member (name, value) = JsonMember name (${scalars[type]} value)`)
  return '(** Generated project field readers and writers from daw.td [DAW-PROJECT-MODEL]. *)\nmodule ProjectCodec\n' +
    [...helpers, ...output].join('\n\n') + '\n'
}
