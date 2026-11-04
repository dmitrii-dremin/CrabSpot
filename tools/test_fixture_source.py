"""Canonical bytes and pre-regeneration provenance regression checks."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from fixture_source import read_java_source
from check_fixture_sources import verify


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
