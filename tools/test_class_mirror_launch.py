from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

from verify_class_mirror_entry import check_api, check_cli, expected_effects


class ClassMirrorLaunchTests(unittest.TestCase):
    def test_returned_field_and_proof_are_both_required(self):
        expected = "returned fields=127/3/1"
        for stdout, valid in ((b"returned fields=127/3/1 proved=21\n", True),
                              (b"returned fields=127/3/0 proved=21\n", False),
                              (b"returned fields=127/3/1 proved=\n", False)):
            with patch("verify_class_mirror_entry.subprocess.run",
                       return_value=subprocess.CompletedProcess([], 0, stdout, b"")):
                if valid:
                    self.assertEqual(check_api([Path("observer")], expected), 21)
                else:
                    with self.assertRaises(ValueError):
                        check_api([Path("observer")], expected)

    def test_image_stage_does_not_accept_an_execution_failure(self):
        result = subprocess.CompletedProcess([], 1, b"", b"crabspot: Dependencies x Some(262): MissingApplicationDependency")
        check_cli("literal/missing-array", result)
        for wrong in (b"crabspot: MainEvaluation: NoClassDefFoundError",
                      b"crabspot: Dependencies x Some(2): MissingApplicationDependency"):
            with self.assertRaises(ValueError):
                check_cli("literal/missing-array", subprocess.CompletedProcess([], 1, b"", wrong))

    def test_native_declaration_has_an_invoke_site_and_no_fabricated_offset(self):
        message = b"crabspot: MainEvaluation: caller at bytecode offset 8: unsupported method body: native methods require runtime services"
        check_cli("call/native-overload", subprocess.CompletedProcess([], 1, b"", message))
        with self.assertRaises(ValueError):
            check_cli("call/native-overload", subprocess.CompletedProcess([], 1, b"", message+b" bytecode offset 0:"))

    def test_new_normal_cases_need_both_reference_results(self):
        case = "call/new"
        good = {"effects/"+case: "returned 2"}
        wrong = {"effects/"+case: "returned 1"}
        self.assertEqual(expected_effects(case, good, good), "returned fields=2")
        with self.assertRaises(ValueError):
            expected_effects(case, good, wrong)


if __name__ == "__main__":
    unittest.main()
