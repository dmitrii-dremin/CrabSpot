import json
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch

import application_entry_reference as reference
from verify_application_entry import check_failure_diagnostic, extract_classes


class ApplicationEntryReferenceTests(unittest.TestCase):
    def test_cli_failures_must_report_the_observed_phase_and_cause(self):
        check_failure_diagnostic("app.Invalid", "image Verification", "crabspot: Verification class invalid")
        expected = "NamedInitialization NullPointerException 14/0"
        detail = "call site at bytecode offset 4: bytecode offset 1: NullPointerException"
        check_failure_diagnostic("named.Broken", expected, "crabspot: NamedInitialization: " + detail)
        for diagnostic in ("crabspot: MainEvaluation: " + detail,
                           "crabspot: NamedInitialization: NullPointerException",
                           "crabspot: NamedInitialization: " + detail.replace("offset 4", "offset 3"),
                           "crabspot: NamedInitialization: " + detail.replace("offset 1", "offset 4"),
                           "crabspot: NamedInitialization: " + detail.replace("NullPointerException", "IllegalAccessError")):
            with self.assertRaises(ValueError):
                check_failure_diagnostic("named.Broken", expected, diagnostic)

    def setUp(self):
        self.scratch = reference.ROOT / "target/fixture-tool"
        self.scratch.mkdir(parents=True, exist_ok=True)

    def test_case_inventories_exclude_the_observer_and_isolate_verification_defects(self):
        with tempfile.TemporaryDirectory(dir=self.scratch) as tmp:
            cases = reference.stage_cases(Path(tmp), reference.DIRECTORY / "classes")
            normal = reference.inventory(cases["ordinary"])
            self.assertEqual(normal.keys(), {"app/" + name + ".class" for name in reference.NORMAL})
            self.assertEqual(reference.inventory(cases["unused-invalid"]).keys() - normal.keys(), {"app/Invalid.class"})
            self.assertEqual(normal.keys() - reference.inventory(cases["missing"]).keys(), {"app/First.class"})
            for case in cases.values():
                self.assertFalse(any(name.startswith(("observer/", "entry/")) for name in reference.inventory(case)))

    def test_invalid_encoding_is_a_single_reproducible_return_instruction_change(self):
        classfile = (reference.DIRECTORY / "classes/app/Invalid.class").read_bytes()
        altered = bytes.fromhex("10 45 b0")
        self.assertEqual(classfile.count(altered), 1)
        valid = classfile.replace(altered, bytes.fromhex("10 45 ac"))
        self.assertEqual(reference.invalidate_return(valid), classfile)
        for malformed in (classfile, valid + valid):
            with self.assertRaisesRegex(ValueError, "expected one"):
                reference.invalidate_return(malformed)

    def test_noncanonical_source_is_rejected_before_tool_discovery_or_execution(self):
        with tempfile.TemporaryDirectory(dir=self.scratch) as tmp:
            folder = Path(tmp)
            (folder / "src").mkdir()
            for ending in (b"\r\n", b"\r"):
                (folder / "src/App.java").write_bytes(b"class App {}" + ending)
                with patch.object(reference, "DIRECTORY", folder), patch.object(reference, "reference_tool") as discover:
                    with self.assertRaisesRegex(ValueError, "must use LF"):
                        reference.record(True, None)
                    discover.assert_not_called()

    def test_pinned_observations_distinguish_preflight_from_partial_application_effects(self):
        for vm in ("zulu", "openj9"):
            rows = json.loads((reference.DIRECTORY / (vm + ".json")).read_bytes())
            self.assertEqual(rows["ordinary"]["once"], [0, "invoke returned 1/1/37/1/0"])
            self.assertEqual(rows["invalid"]["once"], [0, "resolve-entry VerifyError 0/0/0/0/0"])
            self.assertEqual(rows["missing"]["once"], [0, "invoke NoClassDefFoundError 1/0/0/0/0"])
            self.assertEqual(rows["named-direct"]["Child"], [0, ""])
            self.assertEqual(rows["named-direct"]["Parent"], [1, ""])

    def test_platform_stage_writes_only_ordinary_files_under_a_new_directory(self):
        def packed(*names):
            chunks = [struct.pack(">I", len(names))]
            for name in names:
                raw = name.encode()
                chunks.extend((struct.pack(">HI", len(raw), 1), raw, b"X"))
            return b"".join(chunks)

        with tempfile.TemporaryDirectory(dir=self.scratch) as tmp:
            parent = Path(tmp)
            wire = parent / "packed"
            wire.write_bytes(packed("module-info", "java/lang/Object"))
            self.assertEqual(extract_classes(wire, parent / "good"), 2)
            self.assertEqual((parent / "good/java/lang/Object.class").read_bytes(), b"X")
            invalid = [packed("module-info", name) for name in ("../escape", "/escape", "X:/escape", "pkg/CON")]
            invalid.extend([packed("module-info", "MODule-info"), packed("a", "b"),
                            wire.read_bytes()[:-1], wire.read_bytes() + b"extra", struct.pack(">I", 1)])
            for i, data in enumerate(invalid):
                wire.write_bytes(data)
                with self.assertRaises(ValueError):
                    extract_classes(wire, parent / str(i))
            self.assertFalse((parent / "escape.class").exists())
