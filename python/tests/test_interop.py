"""Bidirectional native calls, lifetime, errors and boundary validation."""

import gc
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest
import weakref

import osprey

SUFFIX = ".dylib" if sys.platform == "darwin" else ".so"


SOURCE = '''
extern fn osprey_python_call(module: string, name: string, args: string) -> string
extern fn osprey_python_object(operation: string, objectId: int, name: string, args: string) -> string
fn greeting(name: string) = "hello ${name}"
fn invert(value) = !value
fn scale(value: float) = value * 1.5
fn integer(value: int) = value
fn notify(message: string) = print(message)
fn python(module, name, args) = osprey_python_call(module, name, args)
fn object(operation, objectId, name, args) = osprey_python_object(operation, objectId, name, args)
let started = osprey_python_call("builtins", "setattr", "[null, null, null]")
fn startup() = started
'''


class InteropTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.scratch = tempfile.TemporaryDirectory(prefix="osprey-python-tests-")
        cls.root = Path(cls.scratch.name)
        cls.source = cls.root / "library.osp"
        cls.source.write_text(SOURCE)
        cls.library = osprey.build(cls.source, cls.root / ("library" + SUFFIX))

    @classmethod
    def tearDownClass(cls):
        cls.scratch.cleanup()

    def test_scalars_and_utf8_are_checked_before_native_calls(self):
        lib = self.library
        self.assertEqual(lib.greeting("世界 🐦"), "hello 世界 🐦")
        self.assertIs(lib.invert(True), False)
        self.assertEqual(lib.scale(2.0), 3.0)
        for value in [-(2**63), 2**63 - 1]:
            self.assertEqual(lib.integer(value), value)
        for value in [2**63, -(2**63) - 1, True, "1"]:
            with self.assertRaises((TypeError, OverflowError)):
                lib.integer(value)
        for value in [None, b"bytes", "embedded\0nul"]:
            with self.assertRaises((TypeError, ValueError)):
                lib.greeting(value)
        with self.assertRaises(TypeError):
            lib.invert(1)
        with self.assertRaises(TypeError):
            lib.integer()

    def test_exports_accept_python_keyword_arguments(self):
        self.assertEqual(self.library.greeting(name="Python"), "hello Python")
        self.assertEqual(self.library.scale(value=2.0), 3.0)
        self.assertEqual(self.library.call("greeting", name="named"), "hello named")
        with self.assertRaises(TypeError):
            self.library.greeting("a", name="b")
        with self.assertRaises(TypeError):
            self.library.greeting(missing="a")

    def test_effect_example_runs_with_callbacks_installed_before_entry(self):
        example = Path(__file__).resolve().parents[2] / "examples/python/interop.osp"
        result = subprocess.run([sys.executable, "-m", "osprey", str(example)],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "live: 9.0; mock: test answer")

    def test_python_imports_calls_and_exceptions_are_explicit_json(self):
        call = self.library.python
        self.assertEqual(json.loads(call("math", "sqrt", "[81]")), {"ok": 9.0})
        result = call("json", "loads", '["{\\"a\\":[true,null,\\"é\\"]}"]')
        self.assertEqual(json.loads(result), {"ok": {"a": [True, None, "é"]}})
        for module, name, args, kind in [
            ("math", "sqrt", "[-1]", "ValueError"),
            ("missing_osprey_module", "x", "[]", "ModuleNotFoundError"),
            ("math", "missing", "[]", "AttributeError"),
            ("math", "sqrt", "{}", "TypeError"),
            ("math", "sqrt", "[", "JSONDecodeError"),
            ("builtins", "object", "[]", "TypeError"),
            ("builtins", "exit", "[]", "SystemExit"),
            ("math", "sqrt", "[NaN]", "ValueError"),
            ("math", "isfinite", "[1e999]", "ValueError"),
        ]:
            with self.subTest(kind=kind):
                self.assertEqual(json.loads(call(module, name, args))["error"]["type"], kind)
        self.assertIn("error", json.loads(self.library.startup()))

    def test_callback_and_return_buffers_survive_collection_and_repeated_calls(self):
        call = self.library.python
        saved = call("builtins", "str", '["first"]')
        for number in range(50):
            gc.collect()
            self.assertEqual(json.loads(call("builtins", "str", f"[{number}]")), {"ok": str(number)})
        self.assertEqual(json.loads(saved), {"ok": "first"})

    def test_cross_thread_calls_are_rejected(self):
        failures = []
        def work():
            try:
                self.library.integer(1)
            except RuntimeError as error:
                failures.append(str(error))
        thread = threading.Thread(target=work)
        thread.start()
        thread.join()
        self.assertEqual(len(failures), 1)
        self.assertIn("thread", failures[0])

    def test_objects_roundtrip_without_serializing_or_copying_payloads(self):
        class TensorLike:
            def __init__(self, data):
                self.data = data
            def scaled(self, factor):
                return TensorLike([value * factor for value in self.data])
        original = TensorLike([1, 2, 3])
        handle = self.library.keep(original)
        call = self.library.object
        result = json.loads(call("call", handle, "scaled", '{"args":[{"value":2}]}'))["ok"]
        self.assertEqual(self.library.resolve(result).data, [2, 4, 6])
        self.assertIs(self.library.resolve(handle), original)
        self.assertIn("error", json.loads(call("value", handle, "", "")))
        self.assertEqual(json.loads(call("release", handle, "", "")), {"ok": None})
        with self.assertRaises(KeyError):
            self.library.resolve(handle)
        self.library.release(result)

    def test_object_reference_arguments_kwargs_and_invalid_handles(self):
        value = {"data": [1, 2, 3]}
        reference = self.library.keep(value)
        function = self.library.keep(lambda *, arg: arg)
        request = json.dumps({"kwargs": {"arg": {"ref": reference}}})
        result = json.loads(self.library.object("call", function, "", request))["ok"]
        self.assertIs(self.library.resolve(result), value)
        self.assertEqual(json.loads(self.library.object("call_value", function, "", request)), {"ok": value})
        for op, handle, args in [("getattr", -1, ""), ("call", function, '{"args":[{"ref":-1}]}'),
                                  ("call", function, '{"args":[{}]}'), ("unknown", reference, "")]:
            self.assertIn("error", json.loads(self.library.object(op, handle, "", args)))
        for handle in [reference, function, result]:
            self.library.release(handle)

    def test_released_objects_are_collectible(self):
        class Value:
            pass
        value = Value()
        observed = weakref.ref(value)
        handle = self.library.keep(value)
        del value
        gc.collect()
        self.assertIsNotNone(observed())
        self.library.release(handle)
        gc.collect()
        self.assertIsNone(observed())

    def test_nested_callbacks_copy_their_own_responses(self):
        library = self.library
        callback = library.keep(lambda: json.loads(library.python("math", "sqrt", "[49]")))
        result = json.loads(library.object("call", callback, "", "{}"))["ok"]
        self.assertEqual(library.resolve(result), {"ok": 7.0})
        library.release(result)
        library.release(callback)

    def test_unprintable_exceptions_return_the_fixed_failure_envelope(self):
        class BadError(Exception):
            def __str__(self):
                raise ValueError("broken error formatter")
        def work():
            raise BadError()
        callback = self.library.keep(work)
        result = json.loads(self.library.object("call", callback, "", "{}"))
        self.assertEqual(result["error"]["type"], "RuntimeError")
        self.assertIn("failed to encode", result["error"]["message"])
        self.library.release(callback)

    def test_load_is_cached_and_loaded_output_cannot_be_overwritten(self):
        self.assertIs(osprey.load(self.root / ("library" + SUFFIX)), self.library)
        with self.assertRaises(ValueError):
            osprey.build(self.source, self.root / ("library" + SUFFIX))

    def test_load_hardlink_reuses_the_same_native_image(self):
        original = self.root / ("library" + SUFFIX)
        alias = self.root / ("alias" + SUFFIX)
        os.link(original, alias)
        self.assertIs(osprey.load(alias), self.library)
        with self.assertRaises(ValueError):
            osprey.build(self.source, alias)

    def test_replaced_loaded_path_is_rejected(self):
        original = self.root / ("library" + SUFFIX)
        backup = self.root / ("backup" + SUFFIX)
        replacement = self.root / ("replacement" + SUFFIX)
        os.link(original, backup)
        shutil.copyfile(original, replacement)
        os.replace(replacement, original)
        try:
            with self.assertRaises(RuntimeError):
                osprey.load(original)
        finally:
            os.replace(backup, original)

    def test_ml_uses_the_same_boundary(self):
        source = self.root / "ml.ospml"
        source.write_text('extern osprey_python_call (m : string) (n : string) (a : string) -> string\n'
                          'greet (name : string) = "hi ${name}"\n'
                          'python (m, n, a) = osprey_python_call m n a\n')
        lib = osprey.build(source, self.root / ("ml" + SUFFIX))
        self.assertEqual(lib.greet("ML"), "hi ML")
        self.assertEqual(json.loads(lib.python("math", "sqrt", "[16]")), {"ok": 4.0})

    def test_python_flavor_calls_python_and_is_callable_from_python(self):
        source = Path(__file__).resolve().parents[2] / "examples/python/interop.py"
        library = osprey.build(source, self.root / ("native" + SUFFIX))
        self.assertEqual(library.greeting(name="Python flavor"), "hi Python flavor")
        self.assertEqual(json.loads(library.square_root_json(25.0)), {"ok": 5.0})

    def test_invalid_target_options_and_bridge_signatures_fail_before_link(self):
        accepted = subprocess.run([os.environ["OSPREY"], str(self.source), "--target=python", "--check", "--memory=arc"], capture_output=True, text=True)
        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        bad = self.root / "bad.osp"
        bad.write_text('extern fn osprey_python_call(x: int) -> int\nfn call() = osprey_python_call(1)\n')
        for source, flags, message in [
            (bad, [], "signature"),
            (self.source, ["--memory=gc"], "memory"),
            (self.source, ["--run"], "Python"),
            (self.source, ["--no-ffi"], "FFI"),
        ]:
            result = subprocess.run([os.environ["OSPREY"], str(source), "--target=python", "--check", *flags],
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn(message.lower(), result.stderr.lower())

    def test_python_target_rejects_native_fiber_callbacks(self):
        source = self.root / "fiber.osp"
        for program in ['let task = spawn(fn() => 1)\nprint("${await(task)}")\n',
                        'let queue = Channel(1)\n', 'let later = sleep\nlater(1)\n']:
            source.write_text(program)
            result = subprocess.run([os.environ["OSPREY"], str(source), "--target=python", "--check"],
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("fiber concurrency", result.stderr)

    def test_reserved_bridge_symbols_and_unavailable_host_imports_fail_early(self):
        source = self.root / "reserved.osp"
        for program in ['fn python_install(value: int) = value\n',
                        'fn osprey_python_abi(value: int) = value\n',
                        'fn osprey_python_abi(values: List<int>) = listLength(values)\n',
                        'extern fn host(value: int) -> int\n']:
            source.write_text(program)
            result = subprocess.run([os.environ["OSPREY"], str(source), "--target=python", "--check"],
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Python", result.stderr)


if __name__ == "__main__":
    unittest.main()
