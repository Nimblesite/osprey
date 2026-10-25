# Python application ports

Three reduced application ports run entirely through Osprey's Python flavor and the libraries in `packages/`. They use fixed, small inputs and need no CPython interpreter, installed Python libraries, model downloads or datasets at runtime. [NOTICE.md](NOTICE.md) names the original applications and their changes; [source-audit.json](source-audit.json) pins the reviewed sources. Our implementations use [MIT, NIMBLESITE PTY LTD, Australia](LICENSE).

| Application | Original workflow | Libraries exercised |
| --- | --- | --- |
| [Classifier](classifier/main.py) | micrograd demo: train an MLP with hinge loss, L2 regularization and SGD, then evaluate it | micrograd graph/backpropagation/MLP; sklearn split/scaler/metrics; safetensors serialization/reader |
| [Vector search](vector-search/main.py) | FAISS tutorial: build a flat index, add vectors and query neighbours | faiss L2/IP search/normalization; numpy arrays/broadcasting/reductions; scipy softmax; safetensors round trip |
| [Text evaluation](text-evaluation/main.py) | Google ROUGE application: score predictions and produce a report | rouge_score scorer/stemmer; sacrebleu BLEU/chrF; tokenizers WordPiece; jinja2 report rendering |

Every application also uses the testing package's strict arithmetic policy. Arithmetic faults become failing tests. Assertions run inside registered test cases; a program printing plausible numbers without passing its tests fails the runner.

From the repository root, build and run one application:

```sh
make build
target/release/osprey examples/python/ports/classifier --run
target/release/osprey examples/python/ports/vector-search --run
target/release/osprey examples/python/ports/text-evaluation --run
```

The classifier trains 105 parameters for 30 steps, reduces loss from `0.9596` to `0.0014`, scores `1.0` held-out accuracy and verifies its 912-byte checkpoint. Its independent objective check verifies that a margin greater than one has zero hinge loss. Vector search retrieves each query's originating corpus row, rechecks distances with NumPy and verifies normalization and checkpoint data. Text evaluation reports ROUGE, BLEU, chrF and tokenization statistics; its fixed corpus produces BLEU `38.0909` and chrF `48.3087`. Each committed `expected_output.txt` contains the actual complete transcript, including a positive passing test summary.

Check every application against its golden under default, GC and ARC memory:

```sh
python3 scripts/test-python-app-ports.py
python3 scripts/test-python-app-ports-contracts.py
```

Expect nine `PASS` lines from the application runner and five passing runner regressions. The runner requires zero exit status, exact UTF-8 output bytes, positive passing tests, zero failed/skipped tests and exactly one zero-live-object sentinel under ARC. `--memory default`, `--memory gc`, `--memory arc`, `--app classifier` and `--compiler <path>` focus a run. `make python-app-ports` runs the complete matrix after building the compiler.

After an intentional output change, regenerate only the default golden with `python3 scripts/test-python-app-ports.py --memory default --write-expected`, review the diff, then verify GC and ARC against it. Failed tests cannot be recorded as a golden. These examples demonstrate the listed workflows; they do not implement the original apps' complete command-line interfaces, plotting or file-processing features.
