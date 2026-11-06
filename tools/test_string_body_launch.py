import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from string_activation_reference import DIRECTORY, VALUES, prepare
from type_state_reference import ROOT
from verify_string_body_entry import check, observer, replace_instruction


class StringBodyLaunchTests(unittest.TestCase):
    def test_instruction_mutation_preserves_surrounding_code_and_rejects_ambiguous_sites(self):
        blob = bytes.fromhex("01 00 02 2a b1 00 00 ff")
        recipe = rb"\x00\x02(?P<code>\x2a\xb1)\x00\x00"
        self.assertEqual(replace_instruction(blob, recipe, 1, 172), bytes.fromhex("01 00 02 2a ac 00 00 ff"))
        for data, index, new in ((blob+blob,1,2), (blob[:5],1,2), (blob,2,2), (blob,-1,2), (blob,0,256)):
            with self.assertRaises(ValueError):
                replace_instruction(data, recipe, index, new)

    def test_observer_requires_consistent_proof_authority_and_precise_backing_counters(self):
        sample = ("returned\nfields=3\ndiscovery=181 work=5 backings=2 bytes=2 proved=20 "
                  "witnesses=20 admitted=20 argv=1 args=0 evaluations=1\n")
        def read(text):
            with patch("verify_string_body_entry.process", return_value=text):
                return observer("probe", ".", ".", "compact.EmptyOnly", "none")
        row = read(sample)
        check(row, "returned", "3", (5,2,2), discovery=181)
        for malformed in (sample.replace("witnesses=20","witnesses=19"), sample.replace("work=5","work=-1"),
                          sample.replace("bytes=", "units="), sample.replace("fields=", "results=")):
            with self.assertRaises(ValueError):
                read(malformed)
        for fields, counters in (("7", (5,2,2)), ("3", (4,2,2)), ("3", (5,1,2)), ("3", (5,2,1))):
            with self.assertRaises(ValueError):
                check(row, "returned", fields, counters)

    def test_activation_reference_records_actual_comparator_and_distinct_argument_shapes(self):
        rows = {s+"/"+a:fields+"/java.lang.String$CaseInsensitiveComparator"
                for s in ("original","rebuilt") for a,fields in VALUES.items()}
        for vendor in ("hotspot", "openj9"):
            self.assertEqual(rows, json.loads((DIRECTORY/(vendor+".json")).read_bytes()))
        self.assertEqual(len(set(VALUES.values())), 3)

    def test_missing_activation_archive_fails_before_invoking_javac(self):
        scratch = ROOT/"target/fixture-tool"
        scratch.mkdir(exist_ok=True, parents=True)
        with tempfile.TemporaryDirectory(dir=scratch) as tmp:
            base = Path(tmp)
            with patch("string_activation_reference.DIRECTORY", base/"missing"):
                with self.assertRaisesRegex(ValueError, "activation fixture inventory"):
                    prepare(base/"captured", "missing-javac")


if __name__ == "__main__":
    unittest.main()
