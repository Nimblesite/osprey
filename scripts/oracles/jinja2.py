"""CPython Jinja2 render and value oracles for the supported chat subset."""
import jinja2
import json
from expected_module import write

TEMPLATES = [
    ("hello", "Hello {{ name }}!"),
    ("missing", "{{ missing|default('fallback') }}"),
    ("numeric", "{{ 7 // 3 }}:{{ -7 % 3 }}:{{ 2 + 3 * 4 }}"),
    ("concatenation", "{{ 2 ~ 3 }}"),
    ("truth", "{% if enabled %}yes{% else %}no{% endif %}"),
    ("loop", "{% for n in nums %}{{ loop.index }}={{ n }};{% endfor %}"),
    ("empty loop", "{% for n in [] %}bad{% else %}empty{% endfor %}"),
    ("set", "{% set n = 3 %}{{ n + 1 }}"),
    ("join", "{{ nums|join(',') }}"),
    ("case", "{{ name|upper }}:{{ name|capitalize }}"),
    ("trim", "{{ ' a '|trim }}"),
    ("replace", "{{ name|replace('world', 'reader') }}"),
    ("length", "{{ name|length }}:{{ nums|length }}"),
    ("endpoints", "{{ nums|first }}:{{ nums|last }}"),
    ("method", "{{ name.upper() }}:{{ name.startswith('wo') }}"),
    ("dict", "{{ obj.x }}:{{ obj['x'] }}"),
    ("tests", "{{ enabled is boolean }}:{{ nothing is none }}:{{ name is string }}"),
    ("membership", "{{ 2 in nums }}:{{ 'x' in obj }}"),
    ("slice", "{{ name[1:4] }}:{{ nums[1:]|join(',') }}"),
    ("whitespace", "a\n  {% if enabled %}\nb\n  {% endif %}\nc"),
    ("strip markers", "a {{- name -}} b"),
    ("comment", "a{# ignored #}b"),
    ("namespace", "{% set ns = namespace(n=0) %}{% for x in nums %}{% set ns.n = ns.n + x %}{% endfor %}{{ ns.n }}"),
    ("lower filter", "{{ 'WORLD'|lower }}"),
    ("string filter", "{{ 23|string }}"),
    ("list filter", "{{ name|list|join(',') }}"),
    ("items filter", "{{ obj|items|list|length }}"),
    ("mapped filter", "{{ ['a', 'b']|map('upper')|join(',') }}"),
    ("mapped attribute", "{{ [obj, dict(x=8)]|map(attribute='x')|join(',') }}"),
    ("mapped default", "{{ [obj]|map(attribute='missing', default='fallback')|join(',') }}"),
    ("select truth", "{{ [0, 1, 2]|select|join(',') }}"),
    ("select test", "{{ nums|select('equalto', 2)|join(',') }}"),
    ("reject test", "{{ nums|reject('equalto', 2)|join(',') }}"),
    ("select attribute", "{{ [dict(x=1), dict(x=0)]|selectattr('x')|map(attribute='x')|join(',') }}"),
    ("reject attribute", "{{ [dict(x=1), dict(x=0)]|rejectattr('x')|map(attribute='x')|join(',') }}"),
    ("dict methods", "{{ obj.get('x') }}:{{ obj.get('absent', 9) }}:{{ obj.keys()|join(',') }}:{{ obj.values()|join(',') }}"),
    ("dict iteration", "{% for k, v in obj.items() %}{{ k }}={{ v }}{% endfor %}"),
    ("filtered loop", "{% for n in nums if n > 1 %}{{ n }};{% endfor %}"),
    ("conditional", "{{ 'yes' if enabled else 'no' }}"),
    ("boolean operators", "{{ not false }}:{{ true and false }}:{{ false or 'yes' }}"),
    ("group and subtraction", "{{ (9 - 4) / 2 }}"),
    ("exponent literal", "{{ 1.25e2 }}:{{ 2.5e-1 }}"),
    ("escaped string", "{{ 'a\\nb'|replace('\\n', '-') }}"),
    ("dict literal", "{{ {'x': 1, 'y': 2}['y'] }}"),
    ("stepped slices", "{{ nums[::-1]|join(',') }}:{{ name[::2] }}"),
    ("range function", "{{ range(1, 6, 2)|join(',') }}:{{ range(5, 0, -2)|join(',') }}:{{ range(1, 5, -1)|join(',') }}:{{ range(5, 1, 1)|join(',') }}:{{ range(1, 1, 2)|join(',') }}"),
    ("list equality", "{{ [1, 2] == [1, 2] }}:{{ [1] == [2] }}"),
    ("dict equality", "{{ {'x': 1} == {'x': 1} }}:{{ {'x': 1} == {'x': 2} }}"),
    ("repr containers", "{{ [1, 'x'] }}:{{ {'x': 1} }}"),
    ("string membership", "{{ 'or' in name }}"),
    ("list subscript", "{{ nums[-1] }}"),
]


def main():
    env = jinja2.Environment(trim_blocks=True, lstrip_blocks=True)
    context = dict(name="world", nums=[1, 2, 3], enabled=True, nothing=None, obj={"x": 4})
    cases = [[name, template, env.from_string(template).render(context)] for name, template in TEMPLATES]
    macro_source = "{% macro hi(x) %}Hi {{ x }}{% endmacro %}"
    macro = env.parse(macro_source).body[0]
    write("jinja2", "Jinja2 chat-environment renders over a fixed JSON-like context.",
          {"jinja2": jinja2.__version__}, {
              "RENDERS": cases, "CHARS": list("aé🙂"), "REVERSED": "aé🙂"[::-1],
              "SPLIT": " a\t b\n c ".split(), "SEP_SPLIT": "a,,b,".split(","),
              "CAPITALIZED": "hELLO".capitalize(), "REPR": repr("a\nb"), "BOOL_TEXT": str(True),
              "JSON": json.dumps({"x": 2}), "PRETTY_JSON": json.dumps({"x": 2}, indent=2),
              "PRETTY_LIST_JSON": json.dumps([1, 2], indent=2),
              "FLOORDIV": -7 // 3, "MODULO": -7 % 3,
              "MACRO_SOURCE": macro_source, "MACRO_NAME": macro.name, "MACRO_PARAMS": [arg.name for arg in macro.args],
          })


if __name__ == "__main__":
    main()
