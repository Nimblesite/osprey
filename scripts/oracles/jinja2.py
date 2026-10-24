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
              "FLOORDIV": -7 // 3, "MODULO": -7 % 3,
              "MACRO_SOURCE": macro_source, "MACRO_NAME": macro.name, "MACRO_PARAMS": [arg.name for arg in macro.args],
          })


if __name__ == "__main__":
    main()
