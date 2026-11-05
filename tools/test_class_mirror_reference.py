import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from class_mirror_applications import stage_applications
from class_mirror_encodings import driver_encodings, encodings
from class_mirror_reference import DIRECTORY, ROOT, compare_manifest, replay


class ClassMirrorReferenceTests(unittest.TestCase):
    def test_source_and_instruction_boundaries(self):
        raw = encodings()
        self.assertEqual(len(driver_encodings()), 34)
        self.assertIn(bytes.fromhex("12 02 b0"), raw["literal/class-short/q/Caller.class"])
        self.assertIn(bytes.fromhex("13 01 06 b0"), raw["literal/class-wide/q/Caller.class"])
        self.assertIn(bytes.fromhex("14 01 06 b0"), raw["literal/wrong-category/q/Caller.class"])
        self.assertNotIn(b"java/lang/Class", (DIRECTORY/"classes/simple/Plain.class").read_bytes())

    def test_vendor_and_error_phase_remain_distinguishable(self):
        primary, secondary = [json.loads((DIRECTORY/(v+".json")).read_bytes()) for v in ("hotspot", "openj9")]
        self.assertEqual(len(primary), 78)
        self.assertEqual(primary.keys(), secondary.keys())
        for rows in (primary, secondary):
            self.assertEqual(rows["effects/call/cloneable-array"], "returned 2")
            self.assertEqual(rows["call/visible-array"], "lookup VerifyError")
            self.assertEqual(rows["call/through-object"], "invoke IncompatibleClassChangeError")
            self.assertEqual(rows["effects/literal/missing-I"], "NoClassDefFoundError 1")
            self.assertEqual(rows["effects/literal/arr-of-I"], "returned 2")
        self.assertEqual(primary["effects/new/abandon"], "IllegalAccessError 1")
        self.assertEqual(secondary["effects/new/abandon"], "returned 2")

    def test_staged_products_use_originals_and_exclude_reference_controls(self):
        scratch = ROOT/"target/fixture-tool"
        scratch.mkdir(exist_ok=True, parents=True)
        with tempfile.TemporaryDirectory(dir=scratch) as tmp:
            root = Path(tmp)
            cases = stage_applications(root)
            self.assertEqual(len(cases), 38)
            self.assertEqual(len(list((root/"simple").rglob("*.class"))), 1)
            present, missing = root/"literal/arr-of-I", root/"literal/missing-I"
            for path in ("q/Caller.class", "probe/Entry.class"):
                self.assertEqual((present/path).read_bytes(), (missing/path).read_bytes())
            self.assertTrue((present/"I.class").exists())
            self.assertFalse((missing/"I.class").exists())
            self.assertFalse(any(root.rglob("EntryEffects.class")))
            with self.assertRaises(FileExistsError):
                stage_applications(root)

    def test_cross_compiler_inventory_and_exact_originals(self):
        stored = json.loads((DIRECTORY/"manifest.json").read_bytes())
        current = copy.deepcopy(stored)
        current["compiled"]["simple/Plain.class"] = "different compiler output"
        with self.assertRaisesRegex(ValueError, "compiler output"):
            compare_manifest(current, stored)
        current["compiler"] = "another Java 21 build"
        compare_manifest(current, stored)
        current["support"].pop("p/Face.class")
        with self.assertRaisesRegex(ValueError, "support"):
            compare_manifest(current, stored)
        current = dict(stored, primary="another Java 21 VM", raw={})
        with self.assertRaisesRegex(ValueError, "raw"):
            compare_manifest(current, stored)

    def test_replay_rejects_wrong_stage_missing_row_stderr_and_failed_process(self):
        program = [("call", [Path("app")], ["Observer"])]
        expected = {"call": "invoke IncompatibleClassChangeError"}
        for code, stdout, stderr in ((0, b"lookup IncompatibleClassChangeError", b""),
                                     (0, b"invoke IncompatibleClassChangeError", b"warning"),
                                     (1, b"invoke IncompatibleClassChangeError", b"")):
            with patch("class_mirror_reference.subprocess.run", return_value=subprocess.CompletedProcess(
                [], code, stdout, stderr
            )), self.assertRaises(ValueError):
                replay("java", [], program, expected)
        with self.assertRaisesRegex(ValueError, "inventory"):
            replay("java", [], [], expected)


if __name__ == "__main__":
    unittest.main()
