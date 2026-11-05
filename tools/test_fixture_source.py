"""Canonical bytes and pre-regeneration provenance regression checks."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from fixture_source import read_java_source
from check_fixture_sources import verify
from type_verification_facts import parse_facts
from verify_java_base import pack_archive


class FixtureSourceTests(unittest.TestCase):
    def setUp(self):
        scratch = Path(__file__).resolve().parents[1] / "target" / "fixture-source-tests"
        scratch.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=scratch)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / "tests/fixtures/java/A.java"
        self.source.parent.mkdir(parents=True)
        self.manifest = self.root / "tests/fixtures/classes/manifest.json"
        self.manifest.parent.mkdir(parents=True)
        self.record = self.root / "tests/fixtures/reference/a.json"
        self.record.parent.mkdir(parents=True)
        self.record.with_suffix(".tsv").write_text("I:0\n")
        self.source.write_bytes(b"class A {}\n")
        self.digest = hashlib.sha256(self.source.read_bytes()).hexdigest()
        self.manifest.write_text(json.dumps({"sources": {"A.java": self.digest}}))
        self.record.write_text(json.dumps({"source": "A.java", "source_sha256": self.digest}))

    def test_lf_bytes_are_accepted_without_normalization(self):
        for data in (b"", b"class A {}", b"class A {}\n", b"\n\n", "// snowman: \u2603\n".encode()):
            with self.subTest(data=data):
                self.source.write_bytes(data)
                self.assertEqual(read_java_source(self.source), data)

    def test_jmod_inventory_rejects_malformed_and_duplicate_entries_before_packing(self):
        import io
        import struct
        import warnings
        import zipfile

        archive = self.root / "input.jmod"
        packed = self.root / "packed"

        def sample(names):
            data = io.BytesIO()
            with warnings.catch_warnings(), zipfile.ZipFile(data, "w") as jar:
                warnings.simplefilter("ignore", UserWarning)  # Deliberately duplicate ZIP entry.
                for name in names:
                    jar.writestr(name, b"fixture")
            return b"JM\x01\x00" + data.getvalue()

        descriptor, ordinary = "classes/module-info.class", "classes/java/lang/Object.class"
        valid = sample([descriptor, ordinary])
        archive.write_bytes(valid)
        sha, count, size = pack_archive(archive, packed)
        self.assertEqual((sha, count, size), (hashlib.sha256(valid).hexdigest(), 1, 14))
        self.assertEqual(packed.read_bytes()[:4], struct.pack(">I", 2))
        packed.unlink()
        for bad in (valid[:-1], valid[4:], sample([descriptor]), sample([ordinary]),
                    sample([descriptor, ordinary, ordinary]), sample([descriptor, "../escape.class"]),
                    sample([descriptor, "classes/./Object.class"])):
            with self.subTest(size=len(bad)), self.assertRaises(ValueError):
                archive.write_bytes(bad)
                pack_archive(archive, packed)
            self.assertFalse(packed.exists())

    def test_crlf_and_bare_cr_are_rejected_at_every_boundary(self):
        for data in (b"\r", b"\r\n", b"\rclass A {}", b"class\r A {}", b"class A {}\r", b"class A {}\r\n"):
            with self.subTest(data=data):
                self.source.write_bytes(data)
                with self.assertRaisesRegex(ValueError, "must use LF"):
                    read_java_source(self.source)

    def test_valid_pre_regeneration_provenance(self):
        self.assertEqual(verify(self.root), (1, 1))

    def test_manifest_staleness_and_source_set_changes_are_rejected(self):
        for sources in ({}, {"A.java": "stale"}, {"A.java": self.digest, "B.java": self.digest}):
            with self.subTest(sources=sources):
                self.manifest.write_text(json.dumps({"sources": sources}))
                with self.assertRaisesRegex(ValueError, "manifest source hashes/set"):
                    verify(self.root)

    def test_missing_or_orphan_oracle_provenance_is_rejected_before_regeneration(self):
        original = self.record.read_bytes()
        self.record.unlink()
        with self.assertRaisesRegex(ValueError, "JSON/TSV provenance sets"):
            verify(self.root)
        self.record.write_bytes(original)
        (self.record.parent / "orphan.json").write_text("{}")
        with self.assertRaisesRegex(ValueError, "JSON/TSV provenance sets"):
            verify(self.root)

    def test_unknown_missing_and_stale_oracle_sources_are_rejected(self):
        for record in ({}, {"source": "../outside.java"}, {"source": "A.java", "source_sha256": "stale"}):
            with self.subTest(record=record):
                self.record.write_text(json.dumps(record))
                with self.assertRaises(ValueError):
                    verify(self.root)

    def test_original_construction_provenance_and_artifact_changes(self):
        self.check_original_provenance_and_artifact_changes("construction")

    def test_original_code_validation_provenance_and_artifact_changes(self):
        self.check_original_provenance_and_artifact_changes("code-validation")

    def test_type_state_facts_require_closed_hierarchy_and_distinct_declarations(self):
        def units(name):
            return name.encode("utf-16-be").hex()

        row = ["C", "bootstrap", units("java/lang/Object"), "65", "0", "33", "", "a" * 64,
               "jrt:/java.base/java/lang/Object.class"]
        field = ["F", "1", units("value"), units("I")]
        method = ["M", "1", units("value"), units("()I"), "1"]

        def capture(*rows):
            return parse_facts(("\n".join("\t".join(r) for r in rows) + "\n").encode())

        data = capture(row, field, method)["java/lang/Object"]
        self.assertEqual(data["methods"], [(1, "value", "()I", True)])
        self.assertEqual(data["fields"], [(1, "value", "I")])
        for rows in ((row, row), (row, field, field), (row, method, method),
                     (row, method, field), (row, ["I", units("absent/Interface")]),
                     (row, [*method[:-1], "2"]), ([*row[:1], "application", *row[2:]],)):
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                capture(*rows)

    def test_stale_checked_in_type_state_inputs_and_oracles_fail_preflight(self):
        # Use scratch copies so even failing checks leave the repository artifacts intact.
        import shutil
        project = Path(__file__).resolve().parents[1]
        fixture_root = self.root / "tests/fixtures"
        for name in ("java", "classes", "construction", "code-validation", "type-state",
                     "type-state-facts", "class-proof", "reference"):
            shutil.copytree(project / "tests/fixtures" / name, fixture_root / name, dirs_exist_ok=True)
        # setUp's deliberately minimal example is separate from the real manifest.
        for extra in (self.source, self.record, self.record.with_suffix(".tsv")):
            extra.unlink()
        (self.root / "tools").mkdir()
        for emitter in ("type_state_encodings.py", "class_proof_reference.py"):
            shutil.copyfile(project / "tools" / emitter, self.root / "tools" / emitter)
        verify(self.root)
        for relative in ("type-state/probe/TopReturn.class", "reference/type-state.tsv",
                         "classes/fixtures/TypeStateCases.class", "type-state-facts/requested.txt",
                         "type-state-facts/classes/java/lang/Object.class"):
            with self.subTest(relative=relative):
                path = fixture_root / relative
                original = path.read_bytes()
                path.write_bytes(original + b"x")
                with self.assertRaisesRegex(ValueError, "type-state"):
                    verify(self.root)
                path.write_bytes(original)
        for relative in ("class-proof/GoodThenBad.class", "class-proof/NativeWithBody.class.bin",
                         "class-proof/EmptyPreview.class.bin", "reference/class-proof.tsv"):
            with self.subTest(relative=relative):
                path = fixture_root / relative
                original = path.read_bytes()
                path.write_bytes(original + b"x")
                with self.assertRaisesRegex(ValueError, "class-proof"):
                    verify(self.root)
                path.write_bytes(original)

    def check_original_provenance_and_artifact_changes(self, name):
        directory = self.root / "tests/fixtures" / name
        directory.mkdir()
        artifact = directory / "A.class"
        artifact.write_bytes(bytes.fromhex("cafebabe00000041"))
        manifest = directory / "manifest.json"
        record = {"source": "A.java", "source_sha256": self.digest,
                  "files": {"A.class": hashlib.sha256(artifact.read_bytes()).hexdigest()}}
        manifest.write_text(json.dumps(record), encoding="utf-8")
        self.assertEqual(verify(self.root), (1, 1))
        for changed in ({**record, "source": "missing.java"}, {**record, "source_sha256": "stale"},
                        {**record, "files": {}}, {**record, "files": {"A.class": "stale"}},
                        {**record, "files": {**record["files"], "B.class": "extra"}}):
            with self.subTest(record=changed):
                manifest.write_text(json.dumps(changed), encoding="utf-8")
                with self.assertRaisesRegex(ValueError, f"{name} fixture"):
                    verify(self.root)
        manifest.write_text(json.dumps(record), encoding="utf-8")
        artifact.write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, f"{name} fixture hashes/set"):
            verify(self.root)
        artifact.unlink()
        with self.assertRaisesRegex(ValueError, f"{name} fixture hashes/set"):
            verify(self.root)


if __name__ == "__main__":
    unittest.main()
