import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import monitor_reference as reference
from application_entry_reference import inventory
from monitor_reference import DIRECTORY, ENTRY_CASES, archived_encodings, compare, encodings
from verify_monitor_entry import EXPECTED, check_cli


class MonitorReferenceTests(unittest.TestCase):
    def test_compiler_receives_the_bytes_captured_before_tool_discovery(self):
        scratch = reference.ROOT/"target/fixture-tool"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as temporary:
            fixture = Path(temporary)/"fixture"
            shutil.copytree(DIRECTORY, fixture)
            source = fixture/"src/locks/Child.java"
            original = source.read_bytes()

            def discover(name):
                source.write_bytes(original + b"// changed after capture\n")
                return name

            def inspect(javac, staged, work):
                self.assertEqual((staged/"locks/Child.java").read_bytes(), original)
                self.assertNotEqual(staged, source.parents[1])
                raise RuntimeError("reached staged compiler")

            with patch.object(reference, "DIRECTORY", fixture), patch.object(reference, "reference_tool", discover), \
                 patch.object(reference, "version", return_value="Java 21 Server VM"), \
                 patch.object(reference, "compile_inputs", inspect):
                with self.assertRaisesRegex(RuntimeError, "reached staged compiler"):
                    reference.record(True, None)

    def test_different_compiler_may_change_bytes_but_must_generate_the_same_classes(self):
        stored = json.loads((DIRECTORY/"manifest.json").read_bytes())
        current = dict(stored, compiler="different Java 21 compiler", owners=dict(stored["owners"]))
        name = next(iter(current["owners"]))
        current["owners"][name] = "different bytes"
        reference.check_manifest(current, stored)
        with self.assertRaisesRegex(ValueError, "provenance differs: owners"):
            reference.check_manifest(dict(current, compiler=stored["compiler"]), stored)
        del current["owners"][name]
        with self.assertRaisesRegex(ValueError, "inventory differs: owners"):
            reference.check_manifest(current, stored)

    def test_archives_match_original_application_and_synthetic_programs(self):
        raw = encodings()
        self.assertEqual(len(raw), 45)
        self.assertEqual(archived_encodings(), raw)
        self.assertFalse((DIRECTORY/"encoded/flags-sync-init/locks/Flags.class").exists())
        self.assertEqual(inventory(DIRECTORY/"classes"), json.loads((DIRECTORY/"original-classes.json").read_bytes()))
        self.assertEqual(len(inventory(DIRECTORY/"classes")), 7)
        for case in ENTRY_CASES:
            body = raw[f"entry-{case}/locks/RawEntry.class"]
            self.assertNotIn(b"java/lang/Thread", body)
            self.assertNotIn(b"holdsLock", body)
            self.assertEqual(body[:8], bytes.fromhex("ca fe ba be 00 00 00 41"))

    def test_implementation_choice_is_distinguishable_from_both_jdks(self):
        hotspot, j9 = [json.loads((DIRECTORY/(name+".json")).read_bytes()) for name in ("hotspot","openj9")]
        self.assertEqual(hotspot.keys(), j9.keys())
        self.assertEqual(len(hotspot), 62)
        self.assertEqual(hotspot["effects/leak"][1], "IllegalMonitorStateException 3 false/false")
        self.assertEqual(j9["effects/leak"][1], "returned 3 true/false")
        for vm in [hotspot, j9]:
            self.assertEqual(vm["effects/synchronized-instance-exit"][1], "IllegalMonitorStateException 2 false/false")
            self.assertEqual(vm["observe/Entry"][1], "returned 1/2/1/9 000000")
        self.assertEqual(EXPECTED["synchronized-instance-exit"],
                         "MainEvaluation IllegalMonitorStateException drop@1 via main@20 1")

    def test_comparison_allows_diagnostic_wording_only_on_direct_uncaught_failure(self):
        key = "direct/null-enter"
        expected = {key:[1, "", 'Exception in thread "main" java.lang.NullPointerException: HotSpot detail']}
        compatible = 'Exception in thread "main" java.lang.NullPointerException'
        compare({key:[1,"",compatible]}, expected)
        for code, stdout, stderr in [
            (0, "", compatible), (1, "side effect", compatible),
            (1, "", compatible.replace("NullPointerException", "IllegalMonitorStateException")),
            (1, "", "VM failed to start"),
        ]:
            with self.assertRaises(ValueError):
                compare({key:[code,stdout,stderr]}, expected)
        with self.assertRaises(ValueError):
            compare({}, expected)
        with self.assertRaises(ValueError):
            compare({"observe/Null":[1,"",compatible]}, {"observe/Null":expected[key]})

    def test_cli_checks_phase_cause_and_both_bytecode_locations(self):
        expected = EXPECTED["callee-exit"]
        diagnostic = b"crabspot: MainEvaluation: caller at bytecode offset 16: bytecode offset 1: IllegalMonitorStateException"
        check_cli(expected, subprocess.CompletedProcess([],1,b"",diagnostic))
        for err in (diagnostic.replace(b"16:", b"15:"), diagnostic.replace(b"1:", b"10:"),
                    diagnostic.replace(b"IllegalMonitorState", b"NullPointer"),
                    diagnostic.replace(b"MainEvaluation", b"NamedInitialization")):
            with self.assertRaises(ValueError):
                check_cli(expected, subprocess.CompletedProcess([],1,b"",err))
        check_cli(EXPECTED["crossed"], subprocess.CompletedProcess([],0,b"",b""))
        with self.assertRaises(ValueError):
            check_cli(EXPECTED["crossed"], subprocess.CompletedProcess([],0,b"unexpected",b""))


if __name__ == "__main__":
    unittest.main()
