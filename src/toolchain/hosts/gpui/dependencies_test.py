import gzip
import io
import json
import os
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import time
from threading import Barrier
import unittest
from unittest.mock import patch

import dependencies as source
from compare_script_test import CompareScriptTests  # the compare.sh suite runs with this one


# gzip stamps the current time into its header, so two archives of the same entry built a second
# apart differ, and a test that rebuilds one would trip the checksum it took of the first.
def archive(name="fixture-1.0/Cargo.toml", content=b'license = "MIT"\nold\n', files=None):
    output = io.BytesIO()
    with gzip.GzipFile(fileobj=output, mode="wb", mtime=0) as packed, tarfile.open(fileobj=packed, mode="w") as tar:
        for path, body in (files or {name: content}).items():
            entry = tarfile.TarInfo(path)
            entry.size = len(body)
            tar.addfile(entry, io.BytesIO(body))
    return output.getvalue()


def make_commands(target):
    root = source.HERE.parents[3]
    makefile = "\n".join(path.read_text() for path in [root / "Makefile", *sorted(root.glob("scripts/make/*.mk"))])
    recipe = makefile.split("\n" + target + ":", 1)[1].split("\n\n", 1)[0]
    return [line.strip() for line in recipe.splitlines()[1:] if line.startswith("\t")]


class BackportTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.edits = [{"file": "Cargo.toml", "before": ["old\n"], "after": ["new\n"]}]
        (self.root / "fix.json").write_text(json.dumps(self.edits))
        expected = self.root / "expected"
        expected.mkdir()
        (expected / "Cargo.toml").write_bytes(b'license = "MIT"\nnew\n')
        self.bytes = archive()
        self.package = {"name": "fixture", "version": "1.0", "archiveSha256": source.digest(self.bytes),
                        "treeSha256": source.tree_digest(expected), "edits": "fix.json"}

    def stage(self):
        with patch.object(source, "fetch_archive", return_value=self.bytes):
            return source.stage(self.package, self.root / "sources", self.root)

    def test_applies_backport_preserves_license_and_reuses_verified_source(self):
        result = self.stage()
        self.assertEqual((result / "Cargo.toml").read_bytes(), b'license = "MIT"\nnew\n')
        with patch.object(source, "fetch_archive", side_effect=AssertionError("unexpected fetch")):
            self.assertEqual(source.stage(self.package, self.root / "sources", self.root), result)

    def test_a_restaged_source_keeps_one_fixed_time_so_a_cached_host_build_stays_fresh(self):
        first = self.stage()
        stamps = sorted((path.relative_to(first).as_posix(), path.stat().st_mtime) for path in [first, *first.rglob("*")])
        self.assertEqual({stamp for _, stamp in stamps}, {source.STAGED_MTIME}, "every staged path, patched or not")
        self.assertLess(source.STAGED_MTIME, time.time() - 365 * 24 * 3600, "older than any build output cargo compares")
        shutil.rmtree(first)
        time.sleep(1.1)
        again = self.stage()
        self.assertEqual(sorted((path.relative_to(again).as_posix(), path.stat().st_mtime) for path in [again, *again.rglob("*")]), stamps,
                         "a fresh runner stages the same tree at the same times")

    def test_staging_settles_times_where_utime_cannot_skip_symlinks(self):
        real = source.os.utime

        # Windows' os.utime raises NotImplementedError for follow_symlinks=False.
        def windows_utime(path, times, **options):
            if "follow_symlinks" in options:
                raise NotImplementedError("utime: follow_symlinks unavailable on this platform")
            real(path, times)

        with patch.object(source.os, "utime", windows_utime):
            staged = self.stage()
        self.assertEqual({path.stat().st_mtime for path in [staged, *staged.rglob("*")]}, {source.STAGED_MTIME})

    def test_real_cargo_reuses_restaged_bytes_and_rebuilds_changed_backport_bytes(self):
        (self.root / "fix.json").write_text("[]")
        app = self.root / "app"
        (app / "src").mkdir(parents=True)
        (app / "src/main.rs").write_text('fn main() { println!("{}", fixture::value()); }\n')
        manifest = app / "Cargo.toml"
        env = {**os.environ, "CARGO_TARGET_DIR": str(app / "build")}
        expected = self.root / "rust-expected"
        def stage(value):
            files = {"Cargo.toml": b'[package]\nname="fixture"\nversion="1.0.0"\nedition="2021"\n',
                     "src/lib.rs": f"pub fn value() -> u32 {{ {value} }}\n".encode()}
            for name, body in files.items():
                target = expected / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(body)
            packed = archive(files={f"fixture-1.0.0/{name}": body for name, body in files.items()})
            package = {**self.package, "version": "1.0.0", "treeSha256": source.tree_digest(expected), "archiveSha256": source.digest(packed)}
            destination = source.source_directory(self.root / "sources", [package])
            with patch.object(source, "fetch_archive", return_value=packed):
                return source.stage(package, destination, self.root)
        def run(dependency):
            text = '[workspace]\n[package]\nname="backport-consumer"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nfixture={path=' + json.dumps(str(dependency)) + '}\n'
            if not manifest.exists() or manifest.read_text() != text:
                manifest.write_text(text)
            result = subprocess.run(["cargo", "run", "--offline", "--quiet", "--manifest-path", str(manifest)],
                                    env=env, capture_output=True, text=True, timeout=30, check=True)
            return result.stdout.strip()
        first = stage(1)
        self.assertEqual(run(first), "1")
        binary = app / "build/debug" / ("backport-consumer.exe" if os.name == "nt" else "backport-consumer")
        built = binary.stat().st_mtime_ns
        shutil.rmtree(first)
        self.assertEqual(stage(1), first)
        self.assertEqual(run(first), "1")
        self.assertEqual(binary.stat().st_mtime_ns, built, "identical restaging reuses the compiled host")
        second = stage(2)
        self.assertNotEqual(second, first, "changed verified bytes get a different Cargo source identity")
        self.assertEqual((second / "src/lib.rs").stat().st_mtime, source.STAGED_MTIME)
        self.assertEqual(run(second), "2", "the cached binary must execute the changed dependency")

    def test_rejects_corrupt_archive_and_wrong_patch_preimage_without_partial_source(self):
        self.bytes = b"corrupt"
        with self.assertRaisesRegex(ValueError, "archive checksum mismatch"):
            self.stage()
        self.bytes = archive()
        self.edits[0]["before"] = ["absent\n"]
        (self.root / "fix.json").write_text(json.dumps(self.edits))
        with self.assertRaisesRegex(ValueError, "preimage mismatch"):
            self.stage()
        self.assertFalse((self.root / "sources/fixture-1.0").exists())

    def test_rejects_tampered_cache_or_incorrect_expected_backport_bytes(self):
        result = self.stage()
        (result / "Cargo.toml").write_bytes(b"tampered")
        with self.assertRaisesRegex(ValueError, "source integrity mismatch"):
            self.stage()
        self.package["name"] = "other"
        self.bytes = archive("other-1.0/Cargo.toml")
        self.package["archiveSha256"] = source.digest(self.bytes)
        with self.assertRaisesRegex(ValueError, "source integrity mismatch"):
            self.package["treeSha256"] = "0" * 64
            self.stage()

    def test_rejects_archive_traversal_and_patch_traversal(self):
        self.bytes = archive("fixture-1.0/../escape")
        self.package["archiveSha256"] = source.digest(self.bytes)
        with self.assertRaisesRegex(ValueError, "Invalid dependency source path"):
            self.stage()
        self.bytes = archive()
        self.package["archiveSha256"] = source.digest(self.bytes)
        self.edits[0]["file"] = "../escape"
        (self.root / "fix.json").write_text(json.dumps(self.edits))
        with self.assertRaisesRegex(ValueError, "Invalid dependency source path"):
            self.stage()

    def test_rejects_a_cached_symlink_even_when_the_target_bytes_match(self):
        result = self.stage()
        file = result / "Cargo.toml"
        outside = self.root / "outside.toml"
        outside.write_bytes(file.read_bytes())
        file.unlink()
        file.symlink_to(outside)
        with self.assertRaisesRegex(ValueError, "symlink"):
            self.stage()

    def test_empty_backport_inventory_cannot_report_success(self):
        (self.root / "dependencies.json").write_text('{"packages": []}')
        with patch.object(source, "HERE", self.root):
            with self.assertRaisesRegex(ValueError, "inventory"):
                source.main()

    def test_package_identity_cannot_reuse_a_cache_outside_the_destination(self):
        destination = self.root / "sources"
        destination.mkdir()
        shutil.copytree(self.root / "expected", self.root / "escaped-1.0")
        self.package["name"] = "../escaped"
        with self.assertRaisesRegex(ValueError, "Invalid dependency source path"):
            self.stage()

    def test_inventory_refuses_traversing_name_and_version_before_accepting_manifests(self):
        manifest = self.root / "manifest.toml"
        destination = self.root / "sources"
        for field, value in (("name", "../escaped"), ("version", "1.0/../../escaped")):
            with self.subTest(field=field):
                package = dict(self.package, **{field: value})
                target = destination / f"{package['name']}-{package['version']}"
                manifest.write_text('[patch.crates-io]\n' + json.dumps(package['name']) +
                                    ' = { path = ' + json.dumps(str(target)) + ' }\n')
                with self.assertRaisesRegex(ValueError, "Invalid dependency source path"):
                    source.verify_inventory([package], destination, [manifest])

    def test_concurrent_stagers_both_return_the_same_verified_source(self):
        ready = Barrier(2)

        def fetched(package):
            ready.wait(timeout=5)
            return self.bytes

        with patch.object(source, "fetch_archive", side_effect=fetched):
            with ThreadPoolExecutor(max_workers=2) as pool:
                stages = [self.package, self.root / "sources", self.root]
                results = list(pool.map(lambda _: source.stage(*stages), range(2)))
        self.assertEqual(results[0], results[1])
        self.assertEqual(source.tree_digest(results[0]), self.package["treeSha256"])

    def test_losing_stager_refuses_an_unverified_concurrent_destination(self):
        def unverified_winner(root, target):
            target.mkdir()
            (target / "Cargo.toml").write_bytes(b"tampered")
            raise FileExistsError("concurrent destination")

        with patch.object(Path, "rename", unverified_winner):
            with self.assertRaisesRegex(ValueError, "source integrity mismatch"):
                self.stage()

    def test_gpui_make_target_stages_before_host_cargo(self):
        commands = make_commands("_gpui-test")
        stage = "python3 src/toolchain/hosts/gpui/dependencies.py"
        host = "cd src/toolchain/hosts/gpui && cargo test --release && cargo clippy --release -- -D warnings"
        self.assertIn(stage, commands)
        self.assertIn(host, commands)
        self.assertLess(commands.index(stage), commands.index(host))

    def test_build_and_release_entrypoints_gate_stager_regressions(self):
        guard = "python3 src/toolchain/hosts/gpui/dependencies_test.py"
        for target in ("audit", "_gpui-test"):
            with self.subTest(target=target):
                commands = make_commands(target)
                self.assertIn(guard, commands)
                self.assertLess(commands.index(guard), len(commands) - 1)
        script = (source.HERE / "run.sh").read_text()
        stage = 'python3 "$here/dependencies.py"'
        self.assertNotIn("dependencies_test.py", script)
        self.assertIn(stage, script)
        self.assertLess(script.index(stage), script.index('"$compiler" "$app"'))

    def test_both_workspace_overrides_must_match_the_full_pinned_inventory(self):
        manifests = [self.root / "host.toml", self.root / "reference.toml"]
        target = self.root / "sources/fixture-1.0"
        for file in manifests:
            file.write_text('[patch.crates-io]\nfixture = { path = ' + json.dumps(str(target)) + ' }\n')
        source.verify_inventory([self.package], self.root / "sources", manifests)
        manifests[1].write_text('[patch.crates-io]\nfixture = { path = "elsewhere" }\n')
        with self.assertRaisesRegex(ValueError, "inventory mismatch"):
            source.verify_inventory([self.package], self.root / "sources", manifests)
        with self.assertRaisesRegex(ValueError, "Duplicate.*inventory"):
            source.verify_inventory([self.package, self.package], self.root / "sources", manifests)


if __name__ == "__main__":
    unittest.main()
