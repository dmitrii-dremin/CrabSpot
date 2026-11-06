import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from type_state_reference import ROOT
from verify_class_mirror_entry import expected_effects
from verify_string_entry import physical_entry
from verify_string_equality_entry import COUNTERS, observe, reference_queries


class EqualityLaunchTests(unittest.TestCase):
    def test_proof_counts_and_native_state_must_match_even_on_failure(self):
        counters = dict.fromkeys(COUNTERS, 0)
        counters.update(proved=2, witnesses=2, admitted=2, argv=1, evaluations=1, registered=1)
        def read(data, queries=0):
            lines = ["MainEvaluation <clinit>@11 Arrays 2/3","fields=[0, 1]",
                     " ".join(f"{key}={value}" for key,value in data.items())]
            with patch("verify_string_equality_entry.process", return_value="\n".join(lines)):
                observe("binary","platform","app","Main","none",[],lines[0],[0,1],registered=1,queries=queries)
        read(counters)
        for changes in ({"admitted":1},{"witnesses":0},{"registered":0},{"queries":1},{"work":"-1"},{"argv":0}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                read(counters | changes)

    def test_portable_width_check_skips_only_the_vendor_specific_array_assertion_value(self):
        data = json.loads((ROOT/"tests/fixtures/string-equality-execution/observations.json").read_bytes())
        def observe_java(_vm,_cp,_entry,args,options):
            vendor = "openj9" if "-Xshareclasses:none" in options else "hotspot"
            key = "/".join(args) + ("/-esa" if "-esa" in options else "")
            return data[vendor][key]
        with patch("verify_string_equality_entry.run",side_effect=observe_java) as calls:
            reference_queries("vm","hotspot","a","b",False)
            self.assertEqual(calls.call_count,10)
            calls.reset_mock()
            reference_queries("vm","openj9","a","b",True)
            self.assertEqual(calls.call_count,11)
        corrupted = copy.deepcopy(data)
        corrupted["openj9"]["query/1"] = "2/0/1"
        with patch("verify_string_equality_entry.json.loads",return_value=corrupted), self.assertRaises(ValueError):
            reference_queries("vm","openj9","a","b",True)

    def test_physical_monitor_can_assert_signed_and_large_java_ints(self):
        scratch=ROOT/"target/fixture-tool"
        scratch.mkdir(parents=True,exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as temp:
            base=Path(temp)
            (base/"inputs").mkdir()
            values=[("a",-128),("b",127),("c",511),("d",-(1<<31)),("e",(1<<31)-1)]
            physical_entry(base/"out",base/"inputs","Main","Main",values)
            data=(base/"out/physical/Entry.class").read_bytes()
            self.assertIn(bytes.fromhex("10 80 82"),data)
            self.assertIn(bytes.fromhex("10 7f 82"),data)
            for number in (511,-(1<<31),(1<<31)-1):
                self.assertIn(b"\x03"+number.to_bytes(4,"big",signed=True),data)
            with self.assertRaises(OverflowError):
                physical_entry(base/"invalid",base/"inputs","Main","Main",[("a",1<<31)])

    def test_forname_reaches_the_real_call_after_registration(self):
        self.assertEqual(expected_effects("platform/active-forname",{},{}),
                         "MainEvaluation LayoutMissing forName@0 via forName@0 fields=1")
