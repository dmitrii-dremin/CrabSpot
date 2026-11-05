from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import member_resolution_reference as reference


class MemberResolutionReferenceTests(unittest.TestCase):
    def test_stage_replay_distinguishes_failure_phase_and_retained_state(self):
        text = (reference.DIRECTORY / "dispatch/openj9.tsv").read_text(encoding="utf-8")
        reference.compare_rows("dispatch", text, text)
        for changed in (text.replace("caller-load", "invoke", 1), text.replace("1/1/1", "1/0/1", 1),
                        "\n".join(text.splitlines()[1:])):
            with self.assertRaises(ValueError):
                reference.compare_rows("dispatch", changed, text)

    def test_encoded_inputs_match_individually_pinned_class_hashes(self):
        originals = reference.read_originals()
        for family, build in reference.ENCODINGS.items():
            self.assertEqual(reference.hashes(build()), originals[family]["original_input_classes"])

    def test_crlf_and_bare_cr_java_sources_fail_before_jdk_or_regeneration(self):
        scratch = reference.ROOT / "target/fixture-tool"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as tmp:
            folder = Path(tmp)
            common = folder / "Common.java"
            case = folder / "Fixture.java"
            for path in (common, case):
                for ending in (b"\r\n", b"\r"):
                    common.write_bytes(b"// canonical\n")
                    case.write_bytes(b"// canonical\n")
                    path.write_bytes(b"// invalid" + ending)
                    with patch.object(reference, "DIRECTORY", folder), patch.object(reference, "SHARED", common), patch("subprocess.run") as run:
                        with self.assertRaisesRegex(ValueError, "must use LF"):
                            reference.record(True)
                        run.assert_not_called()

    def test_instruction_comment_cannot_replace_a_verified_constant_pool_tag(self):
        output = b"0: getstatic #4 // Field symbolic/Sub.slot:Lvalue/K;\n"
        with patch("subprocess.run", return_value=subprocess.CompletedProcess([], 0, stdout=output)):
            with self.assertRaisesRegex(ValueError, "expected Fieldref"):
                reference.inspect_symbols("javap", ".", "ordinary")

    def test_failed_class_probe_needs_the_same_class_entry_at_both_member_sites(self):
        listing = (b"  #4 = Fieldref #2.#5 // owner/Late.count:I\n"
                   b"  #8 = Methodref #2.#9 // owner/Late.value:()I\n"
                   b"0: getstatic #4 // Field owner/Late.count:I\n"
                   b"0: invokestatic #8 // Method owner/Late.value:()I\n")
        for data, shared in [(listing, True), (listing.replace(b"#2.#9", b"#3.#9"), False)]:
            with patch("subprocess.run", return_value=subprocess.CompletedProcess([], 0, stdout=data)):
                if shared:
                    sites = reference.inspect_symbols("javap", ".", "missing-class")
                    self.assertEqual({site["owner_index"] for site in sites.values()}, {2})
                else:
                    with self.assertRaisesRegex(ValueError, "share the failed Class entry"):
                        reference.inspect_symbols("javap", ".", "missing-class")
