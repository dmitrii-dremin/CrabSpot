"""Negative and provenance controls for the independent public operand harness."""
import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import verify_operand_entry as run
from type_state_encodings import ClassEncoding

class OperandProductTests(unittest.TestCase):
    def setUp(self):
        parent=Path(os.environ.get('CARGO_TARGET_DIR',str(run.ROOT/'target')))/'operand-harness-tests';parent.mkdir(parents=True,exist_ok=True)
        self.temp=tempfile.TemporaryDirectory(dir=parent);self.addCleanup(self.temp.cleanup);self.work=Path(self.temp.name)
    def row(self,status='returned',fields=None,**changes):
        u={k:0 for k in run.COUNTERS};u.update(proved=54,witnesses=54,admitted=54,argv=1,arguments=34,evaluations=1,discovery=4051,throwable_discovery=18184,diag_work=9624,message_units=7700,messages=240,snapshots=496,frames=1686,backtraces=496,trace_work=7638,started=248,produced=248);u.update(changes)
        return status+'\nfields='+json.dumps(run.MAIN if fields is None else fields)+'\n'+' '.join(f'{k}={u[k]}'for k in run.COUNTERS)+'\n'
    def messages(self):
        return json.loads((run.reference.PACK/'message-oracles.json').read_text(encoding='utf8'))['crabspot']['messages']
    def test_independent_loop_and_prices_give_exact_normal_oracles(self):
        o=run.fixed_oracles(self.messages())
        self.assertEqual(o,dict(messages=240,message_units=7700,diag_work=9624,units=63,temporary=126,snapshots=496,frames=1686,backtraces=496,trace_work=7638,started=248,produced=248))
    def test_message_oracle_changes_are_not_blessed_by_matching_product_output(self):
        for slot in range(17):
            m=self.messages();m[slot]+='!'
            with self.subTest(slot=slot),self.assertRaisesRegex(ValueError,'independent message'):run.fixed_oracles(m)
        m=self.messages();m[11]='operand.Element'
        with self.assertRaises(ValueError):run.fixed_oracles(m)
    def test_normal_requires_exact_fields_counter_set_and_authorities(self):
        run.expect(run.parse_output(self.row()),'returned',run.MAIN,started=248,produced=248,diag_work=9624)
        for changes in ({'proved':0},{'witnesses':53},{'admitted':53},{'argv':0},{'evaluations':2},{'produced':249},{'backtraces':497},{'messages':249}):
            with self.subTest(changes=changes),self.assertRaises(ValueError):run.parse_output(self.row(**changes))
        for value in ('-1','18446744073709551616','２４８'):
            with self.assertRaises(ValueError):run.parse_output(self.row().replace('started=248','started='+value))
        for text in (self.row()+'extra\n',self.row().replace('produced=248','produced=248 extra=0'),self.row().replace('fields=','other='),self.row().replace(str(run.MAIN),'[true]')):
            with self.assertRaises(ValueError):run.parse_output(text)
    def test_wrong_effects_or_original_fault_head_cannot_match(self):
        row=run.parse_output(self.row())
        with self.assertRaises(ValueError):run.expect(row,'returned',[183,*run.MAIN[1:]])
        row=run.parse_output(self.row(run.WRONG,[0]*8))
        with self.assertRaises(ValueError):run.expect(row,run.WRONG.replace('@8','@7'),[0]*8)
        with self.assertRaises(ValueError):run.expect(row,'returned',[0]*8)
    def test_uncaught_and_refilled_heads_are_different_oracles(self):
        a='MainEvaluation uncaught java/lang/ArrayIndexOutOfBoundsException at operand/OperandGuest.loadI([II)I@7'
        b='MainEvaluation uncaught java/lang/ArrayIndexOutOfBoundsException at app/OperandUncaught.main([Ljava/lang/String;)V@143'
        row=run.parse_output(self.row(a,[7,0],started=1,produced=1,messages=1))
        with self.assertRaises(ValueError):run.expect(row,b,[7,0])
        with self.assertRaises(ValueError):run.expect(row,a,[7,9])
    def test_utf16_policy_uses_units_not_unicode_scalar_count(self):
        self.assertEqual(run.units('\U0001f680'),2);self.assertEqual(run.units('\udfff'),1)
        self.assertEqual(run.units('operand.\u00c9l\u00e9ment'),15)
    def test_discovery_reader_has_separate_fixed_cp_role_prices(self):
        blobs={}
        for i,name in enumerate(run.NAMES):
            c=ClassEncoding(name,run.NAMES[0]if i==1 else 'java/lang/RuntimeException')
            blobs[name+'.class']=c.finish([],[(1,'<init>','(Ljava/lang/String;)V',b'\xb1',0,2)])
        # Five self-row/CP costs + each unique-length matching role name;
        # five revisits/parentCP/member/name/descriptor, fixed comparisons.
        expected=15+sum(map(len,run.NAMES))+5*33+4*len('java/lang/RuntimeException')+len(run.NAMES[0])
        self.assertEqual(run.discovery_work(blobs),expected)
        del blobs[run.NAMES[2]+'.class']
        with self.assertRaisesRegex(ValueError,'roles missing'):run.discovery_work(blobs)
    def test_truncated_and_invalid_cp_cannot_be_treated_as_discovery_input(self):
        for raw in (b'',bytes.fromhex('cafebabe000000410002ff'),bytes.fromhex('cafebabe000000410002010010')+b'x'):
            with self.assertRaises(ValueError):run.ClassMeta(raw)
    def test_subprocess_uses_fixed_staged_paths_and_mutation_is_rejected_immediately(self):
        folder=self.work/'staged';folder.mkdir();p=folder/'Guest.class';p.write_bytes(b'fixed');expected=run.simple_hashes(folder)
        def check():
            if run.simple_hashes(folder)!=expected:raise ValueError('staged input changed')
        def mutate(command,**kwargs):
            self.assertEqual(command,['driver',str(folder)])
            self.assertEqual(p.read_bytes(),b'fixed');p.write_bytes(b'mutated')
            return subprocess.CompletedProcess(command,0,b'',b'')
        runner=run.Runner(check)
        with patch.object(run.subprocess,'run',side_effect=mutate),self.assertRaisesRegex(ValueError,'staged input changed'):runner.run(['driver',folder])
        self.assertEqual(runner.rows,[])
    def test_preexisting_drift_prevents_the_next_process(self):
        p=self.work/'Tool';p.write_bytes(b'fixed');h=run.digest(p.read_bytes());p.write_bytes(b'changed')
        def check():
            if run.digest(p.read_bytes())!=h:raise ValueError('binary changed')
        with patch.object(run.subprocess,'run')as child,self.assertRaisesRegex(ValueError,'binary changed'):run.Runner(check).run(['child'])
        child.assert_not_called()
    def test_extra_removed_and_symlinked_staged_inputs_are_not_rebaselined(self):
        folder=self.work/'classes';folder.mkdir();p=folder/'Guest.class';p.write_bytes(b'fixed');saved=run.simple_hashes(folder);extra=folder/'Injected.class';extra.write_bytes(b'extra');self.assertNotEqual(run.simple_hashes(folder),saved);extra.unlink();p.unlink();self.assertNotEqual(run.simple_hashes(folder),saved)
        p.write_bytes(b'fixed')
        real=Path.lstat
        class Reparse:
            st_mode=0o40755;st_file_attributes=0x400
        with patch.object(Path,'lstat',lambda path:Reparse()if path==folder else real(path)),self.assertRaises(ValueError):run.simple_hashes(folder)
    def test_line_ending_only_tool_conversion_preserves_canonical_pin(self):
        p=self.work/'tool.py';p.write_bytes(b'a\nb\n');h=run.canonical_tool(p);p.write_bytes(b'a\r\nb\r\n');self.assertEqual(run.canonical_tool(p),h);p.write_bytes(b'a\rb\n')
        with self.assertRaises(ValueError):run.canonical_tool(p)
    def test_failed_child_still_runs_the_postcheck_and_does_not_publish_success(self):
        checks=[];runner=run.Runner(lambda:checks.append(1))
        with patch.object(run.subprocess,'run',return_value=subprocess.CompletedProcess(['child'],1,b'',b'wrong')),self.assertRaises(ValueError):runner.run(['child'])
        self.assertEqual(checks,[1,1]);self.assertEqual(len(runner.rows),1);self.assertFalse(runner.rows[0]['accepted'])
    def test_partial_progress_is_saved_after_each_successful_command(self):
        report=self.work/'progress.json';runner=run.Runner(lambda:None,report)
        with patch.object(run.subprocess,'run',return_value=subprocess.CompletedProcess(['child'],0,b'fixed',b'')):
            self.assertEqual(runner.run(['child']),'fixed')
        self.assertEqual(json.loads(report.read_text(encoding='utf8'))[0]['stdout'],'fixed')
    def test_setup_partial_strings_are_not_misclassified_as_zero_effect_verification(self):
        u={k:0 for k in run.COUNTERS};u.update(proved=54,witnesses=54,admitted=54,argv=1,arguments=33,string_objects=33)
        row=run.parse_output('image Setup StringObjects 33/34\n'+' '.join(f'{k}={u[k]}'for k in run.COUNTERS)+'\n')
        self.assertIsNone(row['fields']);self.assertEqual(row['usage']['arguments'],33)

    def test_actual_unsupported_npe_text_and_both_instruction_sites_are_required(self):
        text='crabspot: MainEvaluation: owner ClassId { registry: 1, index: 54 } method #13 loadI([II)I: caller owner ClassId { registry: 1, index: 53 } call site method #1 at bytecode offset 112: bytecode offset 7: VM exception production is unsupported: NullPointerException\n'
        run.validate_null_cli(text)
        for wrong in (text.replace('NullPointerException','ArrayIndexOutOfBoundsException'),text.replace('offset 7:','offset 8:'),text.replace('offset 112:','offset 111:'),text.replace('index: 53','index: 54'),text.replace('loadI([II)I','storeI([III)V'),text.replace('VM exception production is unsupported','UnsupportedVmException')):
            with self.assertRaises(ValueError):run.validate_null_cli(wrong)
    def test_snapshot_reads_only_each_actual_bounded_size(self):
        root=self.work/'platform';(root/'java/lang').mkdir(parents=True);(root/'java/lang/Object.class').write_bytes(b'AB');(root/'java/lang/Throwable.class').write_bytes(b'CDE');real=Path.open;requests=[]
        class Reader:
            def __init__(self,inner):self.inner=inner
            def __enter__(self):return self
            def __exit__(self,*args):self.inner.close()
            def read(self,n=-1):requests.append(n);return self.inner.read(n)
        with patch.object(Path,'open',lambda path,*a,**kw:Reader(real(path,*a,**kw))):self.assertEqual(sum(map(len,run.snapshot_directory(root).values())),5)
        self.assertEqual(sorted(requests),[3,4])
    def test_diagnostic_one_short_prices_keep_fixed_work_and_guest_prefix(self):
        first=run.diagnostic_prefix(self.messages(),'diag-work',39);self.assertEqual(first,dict(fields=[0]*8,diag_work=5,message_units=0,messages=0,started=1,produced=0))
        last=run.diagnostic_prefix(self.messages(),'diag-work',9623);self.assertEqual(last,dict(fields=[184,42,6,7,0,239,18,0],diag_work=9569,message_units=7645,messages=239,started=240,produced=239))
        self.assertEqual(run.diagnostic_prefix(self.messages(),'total-units',7699),last)
    def test_streaming_hash_and_snapshot_reject_changed_inventory_without_new_authority(self):
        root=self.work/'platform';(root/'java/lang').mkdir(parents=True);(root/'java/lang/Object.class').write_bytes(b'A');(root/'java/lang/Throwable.class').write_bytes(b'B');captured=run.snapshot_directory(root);pins={k:run.digest(v)for k,v in captured.items()};self.assertEqual(run.snapshot_hashes(root),pins)
        (root/'java/lang/Object.class').write_bytes(b'X');self.assertNotEqual(run.snapshot_hashes(root),pins);self.assertEqual(captured['java/lang/Object.class'],b'A')
    def test_capture_supplied_to_prepare_is_the_original_buffer_object(self):
        captured=run.reference.capture();
        with patch.object(run.reference,'runtime_hashes',return_value={}),patch.object(run.reference.PreparedRun,'compile',return_value='sentinel')as compile_,patch.object(run.reference,'capture',side_effect=AssertionError('must not recapture')):
            self.assertEqual(run.reference.prepare(run.reference.PACK,self.work/'prepare',Path('unused-javac'),captured=captured),'sentinel')
        compile_.assert_called_once()

if __name__=='__main__':unittest.main()
