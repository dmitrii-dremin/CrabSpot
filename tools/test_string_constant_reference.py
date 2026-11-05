import copy
import json
from pathlib import Path
import tempfile
import unittest

from string_applications import applications
from string_constant_encodings import CASES, duplicate_literals, field_readers, literal_class, modified_utf8
from string_constant_reference import DIRECTORY, ROOT, archive, check_manifest, check_transport, matches_observation


class StringReferenceTests(unittest.TestCase):
    def test_modified_utf8_encodes_units_without_normalization(self):
        self.assertEqual(modified_utf8([0, 127, 128, 2047, 2048, 65535]),
                         bytes.fromhex("c0 80 7f c2 80 df bf e0 a0 80 ef bf bf"))
        self.assertEqual(modified_utf8([0xd83d, 0xde80]), bytes.fromhex("ed a0 bd ed ba 80"))
        self.assertEqual(len(modified_utf8([0xdfff]*21845)), 65535)
        for units in ([0xdfff]*21846, [-1], [65536]):
            with self.assertRaises(ValueError):
                modified_utf8(units)

    def test_raw_originals_include_distinct_cp_sites_and_boundaries(self):
        apps, raw = archive(DIRECTORY)
        self.assertEqual(apps["duplicates/Entry.class"], duplicate_literals())
        key = modified_utf8([120, 0, 0xdfff, 49])
        self.assertEqual(duplicate_literals().count(b"\x01"+len(key).to_bytes(2, "big")+key), 2)
        self.assertEqual(set(field_readers()), {"texts/Read.class", "split/Read.class", "constants/Read.class"})
        for name, blob in field_readers().items():
            self.assertEqual(apps[name], blob)
        for name, (units, opcode) in CASES.items():
            self.assertEqual(raw[name+"/sample/Value.class"], literal_class(units, opcode))
        self.assertIn(bytes.fromhex("13 01 04 b0"), raw["isolated-both/sample/Value.class"])
        self.assertIn(bytes.fromhex("14 01 04 b0"), raw["wrong-category/sample/Value.class"])
        self.assertGreater(len(raw["max-mutf/sample/Value.class"]), 65536)
        self.assertFalse((DIRECTORY/"encoded/max-mutf/sample/Value.class").exists())

    def test_stage_excludes_observers_and_preserves_oversize_input(self):
        scratch = ROOT/"target/fixture-tool"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as tmp:
            cases = applications(Path(tmp))
            self.assertEqual(len(cases), 15)
            self.assertEqual(len(list(cases["texts"][0].rglob("*.class"))), 5)
            self.assertFalse(any(Path(tmp).rglob("Observe.class")))
            self.assertEqual(len(list(cases["raw/max-mutf"][0].rglob("*.class"))), 2)
            self.assertGreater((cases["raw/max-mutf"][0]/"sample/Value.class").stat().st_size, 65536)
            with self.assertRaises(FileExistsError):
                applications(Path(tmp))

    def test_manifest_accepts_only_compiler_specific_variations(self):
        original = json.loads((DIRECTORY/"manifest.json").read_bytes())
        current = copy.deepcopy(original)
        current["controls"]["behavior/Observe.class"] = "different"
        with self.assertRaisesRegex(ValueError, "compiler output"):
            check_manifest(current, original)
        current["compiler"] = "other Java 21"
        check_manifest(current, original)
        del current["controls"]["behavior/Observe.class"]
        with self.assertRaisesRegex(ValueError, "compiler output"):
            check_manifest(current, original)
        with self.assertRaisesRegex(ValueError, "provenance"):
            check_manifest(dict(original, original={}), original)

    def test_native_unit_report_checks_delivered_content_and_hash(self):
        row = "5/95/-1940061501/3813251|0063007200610062|0063007200610062|||006100200062"
        delivered = check_transport(row, "a b")
        self.assertEqual(check_transport(row.replace("95/", "76/"), "a b"), delivered)
        # A native launcher may deliver replacement units under its locale.
        self.assertEqual(check_transport(row, "€🚀"), delivered)
        for malformed in ("-001", " 001", "+001", "0x01", "00_1"):
            with self.assertRaisesRegex(ValueError, "invalid native argument report"):
                check_transport(row[:-4] + malformed, "€🚀")
        for changed in (row.replace("95/", "3/"), row.replace("3813251", "3813252"),
                        row.replace("-1940061501", "0"), row[:-4], row.replace("|||", "||")):
            with self.assertRaises(ValueError):
                check_transport(changed, "a b")

    def test_unpinned_build_preserves_canonicality_and_allows_either_first_receiver(self):
        original = json.loads((DIRECTORY/"hotspot.json").read_bytes())
        for label in ("intern/loaders", "intern/unique"):
            expected = original[label]
            width = 2 if label == "intern/loaders" else 1
            changed = "\n".join("/".join(["false"]*width + line.split("/")[width:])
                                for line in expected.splitlines())
            self.assertFalse(matches_observation(label, changed, expected, True))
            self.assertTrue(matches_observation(label, changed, expected, False))
            self.assertFalse(matches_observation(label, changed.replace("/true", "/false", 1), expected, False))
            self.assertFalse(matches_observation(label, changed+"\n"+changed, expected, False))
        expected = original["intern/loaders"]
        self.assertFalse(matches_observation("intern/loaders", expected.replace("true", "false", 1), expected, False))

    def test_vendor_rows_preserve_phase_identity_and_mutated_raw_input_effects(self):
        hs, j9 = [json.loads((DIRECTORY/(v+".json")).read_bytes()) for v in ("hotspot", "openj9")]
        self.assertEqual(hs.keys(), j9.keys())
        for rows in (hs, j9):
            self.assertEqual(rows["entry/normal"], "127/1/1/4/true/true")
            self.assertEqual(rows["fields/constants"], "41/1/41/3")
            self.assertEqual(rows["effects/wrong-category"], "VerifyError/1")
            self.assertEqual(rows["effects/max-mutf"], "returned/3")
        self.assertEqual(hs["raw/wrong-category"], "lookup/VerifyError")
        self.assertEqual(j9["raw/wrong-category"], "load/VerifyError")
        self.assertNotEqual(hs["intern/loaders"], j9["intern/loaders"])


if __name__ == "__main__":
    unittest.main()
