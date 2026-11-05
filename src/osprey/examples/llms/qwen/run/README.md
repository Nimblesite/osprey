# Qwen3.8-27B on Metal

The real model on the real weights, greedy, on GPU buffers.

```
G=~/.cache/osprey-llm/gguf/Qwen3.8-27B-UD-Q4_K_XL.gguf
P=~/.cache/osprey-llm/oracle-venv/bin/python
IDS=$($P -I prompt_gen.py $G encode "The capital of France is")
($P -I table_gen.py $G "$IDS"; grep -v '^GGUF = \|^# osprey: flavor' qwen.py) > /tmp/q.py
OSPREY_GPU=metal osprey /tmp/q.py --run --memory=arc
$P -I prompt_gen.py $G decode <the ids it printed>
```

The table and the prompt come from Python until `@osprey/gguf` and `@osprey/tokenizers` read them
in this program. It needs about 18 GB of the machine's memory for the wired weights.
