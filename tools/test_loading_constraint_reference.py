import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from loading_constraint_encodings import build
from loading_constraint_reference import DIRECTORY, check_selection, compare_originals, normalize, record


class LoadingReferenceTests(unittest.TestCase):
    def test_each_java_source_rejects_crlf_or_bare_cr_before_jdk_execution(self):
        names = ["LoadingConstraintProbe.java", "LoadingSelectionEdges.java"]
        scratch = Path(__file__).resolve().parents[1] / "target/fixture-tool"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as tmp:
            directory = Path(tmp)
            for rejected in names:
                for ending in (b"\r\n", b"\r"):
                    for name in names:
                        (directory / name).write_bytes(b"// probe" + (ending if name == rejected else b"\n"))
                    with patch("loading_constraint_reference.DIRECTORY", directory), patch("subprocess.run") as run:
                        with self.assertRaisesRegex(ValueError, "must use LF"):
                            record(True)
                        run.assert_not_called()

    def test_both_observed_method_orders_have_one_portable_result(self):
        data = (DIRECTORY / "results.tsv").read_bytes()
        rows = [line.split("\t") for line in data.decode().splitlines()]
        for result in ["B", "LinkageError"]:
            for row in rows:
                if row[0] == "method-order":
                    row[-1] = result
            self.assertEqual(normalize(rows), data)
        with self.assertRaises(ValueError):
            normalize(rows[:-1])
        rows[-1][-1] = "C"
        with self.assertRaises(ValueError):
            normalize(rows)

    def test_original_class_bytes_and_stage_order_are_independent_checks(self):
        files = build()
        rows = [line.split("\t") for line in (DIRECTORY / "results.tsv").read_text(encoding="utf-8").splitlines()]
        origins = json.loads((DIRECTORY / "development.json").read_text(encoding="utf-8"))
        compare_originals(files, rows, origins)
        with self.assertRaisesRegex(ValueError, "original class bytes differ"):
            compare_originals({**files, "value/K": files["value/P"]}, rows, origins)
        rows[0], rows[1] = rows[1], rows[0]
        with self.assertRaisesRegex(ValueError, "original JDK stages differ"):
            compare_originals(files, rows, origins)

    def test_other_jvm_can_enforce_required_constraint_at_either_link_trigger(self):
        original = ["Prot", "split", "initialize", "ok", "invoke", "IllegalAccessError", "A", "C"]
        contract = ("A", "C", "equal")
        self.assertFalse(check_selection(original, original, contract, True))
        for trigger in ["define", "initialize"]:
            normative = ["Prot", "split", trigger, "LinkageError", trigger, "LinkageError", "A", "C"]
            self.assertTrue(check_selection(normative, original, contract, True))
        with self.assertRaisesRegex(ValueError, "unclassified"):
            check_selection([*original[:3], "VerifyError", *original[4:]], original, contract, True)

    def test_late_binding_and_invocation_cannot_hide_an_unrelated_failure(self):
        original = ["Abs", "late", "initialize", "ok", "A", "C"]
        contract = ("A", "C", "equal")
        self.assertTrue(check_selection([*original[:-1], "LinkageError"], original, contract, False))
        with self.assertRaisesRegex(ValueError, "invalid selection"):
            check_selection(original[:-1], original, contract, False)
        original = ["Abs", "shared", "initialize", "ok", "invoke", "AbstractMethodError", "A", "A"]
        with self.assertRaisesRegex(ValueError, "unexpected invocation"):
            check_selection([*original[:5], "ok", *original[6:]], original, contract, True)
