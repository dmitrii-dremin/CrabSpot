import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from application_entry_reference import inventory
from string_body_nests import CASES, make
from string_body_reference import DIRECTORY, ROOT, capture_inputs, check_provenance, compile_captured


class StringBodyReferenceTests(unittest.TestCase):
    def test_changes_after_capture_cannot_replace_compiler_or_runtime_inputs(self):
        scratch = ROOT/"target/fixture-tool"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as tmp:
            base = Path(tmp)
            live = base/"live"
            names = ("src/A.java", "controls/B.java", "classes/A.class", "encoded/variant/A.class")
            for name in names:
                path = live/name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(name.encode())
            captured = base/"captured"
            hashes = capture_inputs(live, captured)
            for name in names:
                (live/name).write_bytes(b"changed")
            (live/"src/Inserted.java").write_bytes(b"inserted")
            with patch("string_body_reference.subprocess.run") as run:
                compile_captured("javac", captured, hashes, base/"classes", base/"controls")
            sources = [arg for call in run.call_args_list for arg in call.args[0] if str(arg).endswith(".java")]
            self.assertEqual(sources, [captured/"src/A.java", captured/"controls/B.java"])
            for name in names:
                self.assertEqual((captured/name).read_bytes(), name.encode())
                group, key = name.split("/", 1)
                self.assertEqual(hashlib.sha256((captured/name).read_bytes()).hexdigest(), hashes[group][key])

    def test_all_nest_attributes_reencode_to_the_archived_bytes(self):
        scratch = ROOT/"target/fixture-tool"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as tmp:
            for case in CASES:
                folder = make(Path(tmp), *case)
                self.assertEqual(inventory(folder), inventory(DIRECTORY/"encoded"/case[0]))
        self.assertEqual(len(CASES), 9)

    def test_nest_access_requires_both_loader_and_metadata_identity(self):
        for vendor in ("hotspot", "openj9"):
            rows = json.loads((DIRECTORY/(vendor+".json")).read_bytes())
            self.assertEqual(rows["nest/split-loader"], "true/true/41\nfalse/false/IllegalAccessError")
            self.assertEqual(rows["extra-missing/prior-query"], "peer.Host/true/returned/7")
            self.assertEqual(rows["other-host/prior-query"], "peer.Other/false/IllegalAccessError/1")
            for case in ("no-host", "no-members", "neither", "absent-host", "other-package", "unlisted"):
                self.assertEqual(rows[case+"/prior-query"], "peer.Host$Inner/false/IllegalAccessError/1")

    def test_pinned_layout_and_public_utf16_observations(self):
        for vendor in ("hotspot", "openj9"):
            rows = json.loads((DIRECTORY/(vendor+".json")).read_bytes())
            self.assertEqual(rows["strings/arguments"], "127/0/3/4/1/2/3")
            layout = json.loads((DIRECTORY/(vendor+"-layout.json")).read_bytes())
            self.assertIn("3/0/3/4100ff", layout["default"].splitlines())
            self.assertIn("1/1/2/ffdf", layout["default"].splitlines())
            self.assertIn("[Ljava.io.ObjectStreamField;/0", layout["default"].splitlines())
            self.assertIn("java.lang.String$CaseInsensitiveComparator/true/true/true", layout["default"].splitlines())
            if vendor == "hotspot":
                self.assertIn("3/1/6/41000000ff00", layout["wide"].splitlines())

    def test_another_compiler_cannot_remove_classes_or_change_archived_input(self):
        base = json.loads((DIRECTORY/"manifest.json").read_bytes())
        current = copy.deepcopy(base)
        current["compiled"]["compact/Entry.class"] = "changed"
        with self.assertRaisesRegex(ValueError, "javac output"):
            check_provenance(current, base)
        current["compiler"] = "different javac"
        check_provenance(current, base)
        del current["compiled"]["compact/Entry.class"]
        with self.assertRaisesRegex(ValueError, "javac output"):
            check_provenance(current, base)
        with self.assertRaisesRegex(ValueError, "provenance"):
            check_provenance(dict(base, sources={}), base)
        with self.assertRaisesRegex(ValueError, "schema"):
            check_provenance(dict(base, extra=1), base)


if __name__ == "__main__":
    unittest.main()
