"""Negative checks for the public Throwable replay comparator and captured inputs."""
import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import verify_throwable_entry as run


class PublicThrowableHarnessTests(unittest.TestCase):
    def setUp(self):
        target=Path(os.environ.get('CARGO_TARGET_DIR',str(Path(__file__).resolve().parents[1]/'target')))/'throwable-tool-tests'
        target.mkdir(parents=True,exist_ok=True)
        self.temp=tempfile.TemporaryDirectory(prefix='throwable-comparator-',dir=target)
        self.addCleanup(self.temp.cleanup)
        self.work=Path(self.temp.name)

    def row(self,status='returned',fields=None,**changes):
        usage=dict(zip(run.COUNTERS,(50,50,50,1,0,1,8000,23,51,23,468,13,13)))
        usage.update(changes)
        if fields is None:fields=run.MAIN
        return status+'\nfields='+json.dumps(fields)+'\n'+' '.join(f'{key}={usage[key]}' for key in run.COUNTERS)+'\n'

    def test_exact_normal_fields_and_all_counter_names_are_required(self):
        row=run.parse_output(self.row())
        run.expect(row,'returned',[65535,8,4,1],started=13,produced=13,frames=51)
        with self.assertRaises(ValueError):run.expect(row,'returned',[65534,8,4,1])
        with self.assertRaises(ValueError):run.expect(row,'returned',[65535,8,4,1],produced=12)
        for changed in (self.row().replace('started=13','started=13 extra=0'),
                        self.row().replace('proved=50','proved=-50'),
                        self.row().replace('proved=50','proved=18446744073709551616'),
                        self.row().replace('proved=50','proved=５0')):
            with self.subTest(changed=changed):
                with self.assertRaises(ValueError):run.parse_output(changed)

    def test_proof_witness_layout_or_evaluation_mismatch_is_not_success(self):
        for changes in ({'proved':0},{'witnesses':49},{'admitted':49},{'argv':0},
                        {'evaluations':2},{'produced':14},{'backtraces':24}):
            with self.subTest(changes=changes):
                with self.assertRaises(ValueError):run.parse_output(self.row(**changes))

    def test_unknown_rows_missing_fields_and_invalid_field_types_are_rejected(self):
        for text in (self.row()+'extra\n',self.row().replace('fields=','oops='),
                     self.row().replace('[65535, 8, 4, 1]','[true]'),
                     self.row().replace('[65535, 8, 4, 1]','{}')):
            with self.assertRaises(ValueError):run.parse_output(text)

    def test_wrong_guard_current_head_cannot_be_replaced_by_old_creation_head(self):
        row=run.parse_output(self.row(run.WRONG,[65535,8,4,0]))
        run.expect(row,run.WRONG,[65535,8,4,0])
        bad=copy.deepcopy(row)
        bad['status']=bad['status'].replace('app/ThrowableMain.monitorRefill(Ljava/lang/Throwable;)Z@49',
                'probe/ThrowableEntryGuest.makeUser()Lprobe/ThrowableEntryGuest$UserFailure;@11')
        with self.assertRaises(ValueError):run.expect(bad,run.WRONG,[65535,8,4,0])

    def test_uncaught_original_fault_and_refill_head_are_distinct(self):
        row=run.parse_output(self.row(run.UNCAUGHT,[7,0],started=1,produced=1))
        run.expect(row,run.UNCAUGHT,[7,0])
        for status in (run.UNCAUGHT.replace('@8','@9'),run.REFILLED,
                       run.UNCAUGHT.replace('idiv(II)I@8','<init>(Ljava/lang/String;)V@0')):
            with self.assertRaises(ValueError):run.expect(row,status,[7,0])

    def test_stack_trace_native_failure_must_preserve_pc31_and_fields(self):
        row=run.parse_output(self.row(run.TRACE,[7,0],started=0,produced=0))
        run.expect(row,run.TRACE,[7,0])
        with self.assertRaises(ValueError):run.expect(row,'returned',[7,0])
        with self.assertRaises(ValueError):run.expect(row,run.TRACE.replace('@31','@30'),[7,0])
        with self.assertRaises(ValueError):run.expect(row,run.TRACE,[7,1])
        for wrong in ('owner=false','call-owner-same=false','call-method-distinct=false'):
            bad=copy.deepcopy(row)
            key=wrong.split('=')[0]
            bad['status']=bad['status'].replace(key+'=true',wrong)
            with self.assertRaises(ValueError):run.expect(bad,run.TRACE,[7,0])

    def test_image_preflight_requires_every_effect_and_proof_counter_zero(self):
        counters={key:0 for key in run.COUNTERS}
        counters['discovery']=3
        def row(values):
            return 'image Verification DiscoveryWork 3/4\n'+' '.join(f'{k}={values[k]}' for k in run.COUNTERS)+'\n'
        self.assertIsNone(run.parse_output(row(counters))['fields'])
        for key in run.COUNTERS:
            if key=='discovery':continue
            with self.subTest(counter=key):
                bad=counters.copy();bad[key]=1
                with self.assertRaises(ValueError):run.parse_output(row(bad))

    def test_process_checks_exit_stderr_and_does_not_accept_wrong_success(self):
        class Result:
            returncode=0
            stdout=b''
            stderr=b''
        with patch.object(run.subprocess,'run',return_value=Result()) as tool:
            self.assertEqual(run.process(['fixture']), '')
            tool.assert_called_once()
            with self.assertRaises(ValueError):run.process(['fixture'],1,'ArithmeticException')
        Result.returncode=1;Result.stderr=b'other error'
        with patch.object(run.subprocess,'run',return_value=Result()):
            with self.assertRaises(ValueError):run.process(['fixture'],1,'ArithmeticException')

    def platform(self):
        root=self.work/'platform'
        run.stage({'java/lang/Object.class':b'object','java/lang/Throwable.class':b'throwable'},root)
        return root

    def test_platform_is_read_once_then_only_captured_buffers_are_staged(self):
        root=self.platform();captured=run.snapshot_directory(root)
        (root/'java/lang/Throwable.class').write_bytes(b'changed-live-source')
        staged=run.stage(captured,self.work/'staged')
        self.assertEqual((staged/'java/lang/Throwable.class').read_bytes(),b'throwable')

    def test_platform_actual_bytes_and_file_caps_fail_before_staging(self):
        root=self.platform()
        for kwargs in ({'max_files':1},{'max_bytes':5}):
            with self.assertRaisesRegex(ValueError,'snapshot limit'):run.snapshot_directory(root,**kwargs)
        (root/'java/lang/Throwable.class').unlink()
        with self.assertRaisesRegex(ValueError,'incomplete'):run.snapshot_directory(root)

    def test_reparse_or_link_root_is_rejected_before_read(self):
        root=self.platform();real=Path.lstat
        class Reparse:
            st_mode=0o40755
            st_file_attributes=0x400
        with patch.object(Path,'lstat',lambda p:Reparse() if p==root else real(p)):
            with self.assertRaisesRegex(ValueError,'linked/reparse'):run.snapshot_directory(root)

    def test_native_locator_rejects_truncation_unknown_tags_and_wrong_role(self):
        for raw in (b'',bytes.fromhex('cafebabe000000410002ff'),
                    bytes.fromhex('cafebabe000000410002010010')+b'x'):
            with self.assertRaises(ValueError):run.native_flags_offset(raw)

    def test_genuine_fixture_provenance_is_checked_before_reference_process(self):
        with patch.object(run.reference,'capture',side_effect=ValueError('changed fixture')), \
                patch.object(run.reference,'process') as vm:
            with self.assertRaisesRegex(ValueError,'changed fixture'):
                run.reference.prepare(self.work,self.work/'output',Path('missing-javac'))
            vm.assert_not_called()

    def test_reference_comparator_cannot_bless_wrong_guest_observation(self):
        with patch.object(run.reference,'tool_version',return_value='openjdk version "21"'), \
                patch.object(run.reference,'process',return_value=('65534/8/4/1','')):
            with self.assertRaisesRegex(ValueError,'independent'):
                run.independent_references(Path('java'),None,self.work,self.work,self.work)




class CapturedMutationTests(unittest.TestCase):
    def setUp(self):
        target=Path(os.environ.get('CARGO_TARGET_DIR',str(run.ROOT/'target')))/'throwable-mutation-tests'
        target.mkdir(parents=True,exist_ok=True)
        self.temp=tempfile.TemporaryDirectory(dir=target)
        self.addCleanup(self.temp.cleanup)
        self.work=Path(self.temp.name)

    def test_subprocess_guest_control_and_platform_mutations_are_rejected_at_final_check(self):
        class Result:
            returncode=0
            stdout=b''
            stderr=b''
        for label in ('guest','control','platform'):
            directory=self.work/label;directory.mkdir()
            path=directory/'Input.class';path.write_bytes(b'checked')
            expected=[(directory,run.reference.hashes(directory))]
            def mutate(*args,**kwargs):
                path.write_bytes(b'changed by subprocess')
                return Result()
            with patch.object(run.subprocess,'run',side_effect=mutate):
                run.process(['subprocess'])
            with self.assertRaisesRegex(ValueError,'captured input'):run.verify_directories(expected)

    def test_added_or_removed_staged_class_cannot_pass_same_inventory(self):
        directory=self.work/'classes';directory.mkdir()
        path=directory/'Input.class';path.write_bytes(b'checked')
        expected=[(directory,run.reference.hashes(directory))]
        extra=directory/'Injected.class';extra.write_bytes(b'extra')
        with self.assertRaises(ValueError):run.verify_directories(expected)
        extra.unlink();path.unlink()
        with self.assertRaises(ValueError):run.verify_directories(expected)

    def test_helper_binary_or_fixed_live_source_change_is_not_a_new_baseline(self):
        for label in ('helper.py','java.exe','source.java'):
            path=self.work/label;path.write_bytes(b'checked')
            expected={path:run.digest(b'checked')}
            path.write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError,'captured tool or input'):run.verify_files(expected)

    def test_profile_mutation_is_restored_even_if_native_command_fails(self):
        platform=self.work/'platform'
        original=b'fillInStackTrace'+bytes(30)
        run.stage({'java/lang/Throwable.class':original},platform)
        usage={key:0 for key in run.COUNTERS}
        disabled='image Dependencies UnsupportedPlatformReference\n'+' '.join(f'{k}={usage[k]}' for k in run.COUNTERS)+'\n'
        with patch.object(run,'native_flags_offset',return_value=20), \
                patch.object(run,'process',side_effect=[disabled,ValueError('native subprocess failed')]):
            with self.assertRaisesRegex(ValueError,'native subprocess failed'):
                run.profile_controls(Path('driver'),platform,self.work,{'java/lang/Throwable.class':original})
        self.assertEqual((platform/'java/lang/Throwable.class').read_bytes(),original)

    def test_independent_vm_mutation_of_control_is_detected_after_correct_observations(self):
        controls=self.work/'controls';controls.mkdir()
        path=controls/'Observer.class';path.write_bytes(b'checked')
        expected=[(controls,run.reference.hashes(controls))]
        def vm(command):
            path.write_bytes(b'changed by reference VM')
            return run.reference.EXPECTED[command[-1]],''
        with patch.object(run.reference,'tool_version',return_value='openjdk version "21"'), \
                patch.object(run.reference,'process',side_effect=vm):
            rows=run.independent_references(Path('java'),None,controls,self.work,self.work)
        self.assertEqual(len(rows),20)
        with self.assertRaises(ValueError):run.verify_directories(expected)

if __name__=='__main__':unittest.main()
