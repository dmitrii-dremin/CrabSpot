import unittest
from pathlib import Path
from unittest.mock import patch

import json

from string_equality_reference import APPS, CONTROLS, expected, portable, prepare, recording_pins, validate


class EqualityReferenceTest(unittest.TestCase):
    def test_rerecord_refuses_a_changed_class_or_classloader_even_when_string_matches(self):
        pins = {"hotspot": ["String=a", "StringLatin1=b", "Class=c", "ClassLoader=d"]}
        with patch.object(Path, "exists", return_value=True), \
             patch.object(Path, "read_bytes", return_value=json.dumps({"platform_classes":pins}).encode()):
            self.assertEqual(recording_pins(Path("fixture"), pins), pins)
            for index in (2, 3):
                changed = list(pins["hotspot"])
                changed[index] += "0"
                with self.assertRaisesRegex(ValueError, "archived recording pins"):
                    recording_pins(Path("fixture"), {"hotspot":changed})

    def test_portable_rows_ignore_private_layout_but_enforce_java_results_and_loader_identity(self):
        pinned = expected("hotspot", "system-on")
        alternate = expected("openj9", "system-on")
        validate(portable(alternate), pinned, False)
        with self.assertRaises(ValueError):
            validate(alternate, pinned, True)
        for index, value in ((0, "510/10"), (1, "policy=2/2/0/0"), (2, "loaders=1/1/0/1"),
                             (3, "child=1/1/1/1/2")):
            rows = portable(pinned)
            rows[index] = value
            with self.assertRaises(ValueError):
                validate(rows, pinned, False)

    def test_vendor_state_observed_after_init_differs_while_user_equality_agrees(self):
        zulu, j9 = (expected(vendor, "system-on") for vendor in ("hotspot", "openj9"))
        self.assertEqual(zulu[0], "511/10")
        self.assertEqual(zulu[0], j9[0])
        self.assertEqual(zulu[1], "policy=3/2/0/0/1/0")
        self.assertEqual(j9[1], "policy=3/2/0/0/1/1")
        self.assertEqual(zulu[3], "arrays=0/0/0")
        self.assertEqual(j9[3], "arrays=1/0/1")
        self.assertNotEqual(zulu[4], j9[4])

    def test_default_system_application_and_last_switch_are_distinct(self):
        for vendor in ("hotspot", "openj9"):
            self.assertEqual(expected(vendor, "default"), expected(vendor, "system-reset"))
            self.assertNotEqual(expected(vendor, "default"), expected(vendor, "app-class-on"))
            self.assertNotEqual(expected(vendor, "all"), expected(vendor, "all-except-app-class"))
            self.assertNotEqual(expected(vendor, "system-on"), expected(vendor, "system-except-helper"))
            self.assertEqual(expected(vendor, "default")[-2:], ["child=1/1/1/0/2", "child=1/0/0/1/0"])

    def test_missing_application_or_contaminated_archive_fails_before_compilation(self):
        base = Path("unused")
        good = {"src": {s+".java" for s in APPS}, "classes": {s+".class" for s in APPS},
                "controls": {s+".java" for s in CONTROLS}, "encoded": set()}
        for key, changed in (("src", set()), ("controls", {"ReadEquality.java"}),
                             ("classes", good["classes"] | {"Injected.class"}),
                             ("encoded", {"app/Policy.class"})):
            with self.subTest(key=key), \
                 patch("string_equality_reference.capture_inputs", return_value=good | {key:changed}), \
                 patch("string_equality_reference.compile_captured") as compile_guest:
                with self.assertRaisesRegex(ValueError, f"unexpected equality fixture {key} inventory"):
                    prepare(base, base, "no-javac")
                compile_guest.assert_not_called()
