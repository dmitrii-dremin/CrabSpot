"""Hostile-input and comparator guards for the portable operand archive."""
import copy
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import operand_entry_reference as ref


class ReferenceGuards(unittest.TestCase):
    def setUp(self):
        scratch=ref.ROOT / 'target/fixture-tool'
        scratch.mkdir(parents=True,exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix='operand-reference-test-',dir=scratch)
        self.root = Path(self.temp.name)
        self.pack = self.root / 'tests/fixtures/operand-entry'
        shutil.copytree(ref.PACK, self.pack)
        shutil.copytree(ref.ROOT / 'tools', self.root / 'tools', ignore=shutil.ignore_patterns('__pycache__'))

    def tearDown(self):
        self.temp.cleanup()

    def capture(self):
        return ref.capture(self.pack, self.root)

    def reject(self):
        with self.assertRaises((ValueError, KeyError, json.JSONDecodeError)):
            self.capture()

    def test_positive_capture_exact_buffers_and_metadata(self):
        captured = self.capture()
        captured.check()
        self.assertEqual(len(ref.SOURCES), 7)
        self.assertEqual(len(ref.CLASSES), 8)
        self.assertEqual(len(ref.product_arguments(captured)), 34)
        self.assertEqual(len(ref.required_sites()), 40)

    def test_extra_and_missing_classes(self):
        path = self.pack / 'classes/extra.class'
        path.write_bytes(b'wrong')
        self.reject()
        path.unlink()
        (self.pack / 'classes/operand/Tag.class').unlink()
        self.reject()

    def test_source_bytes_and_line_endings_are_exact(self):
        path = self.pack / 'source/app/OperandMain.java'
        path.write_bytes(path.read_bytes().replace(b'\n', b'\r\n'))
        self.reject()

    def test_extra_and_missing_sources(self):
        path = self.pack / 'source/Other.java'
        path.write_bytes(b'class Other{}\n')
        self.reject()
        path.unlink()
        (self.pack / 'source/operand/Tag.java').unlink()
        self.reject()

    def test_class_observer_and_metadata_mutations(self):
        for name in ['classes/operand/Tag.class', 'controls/ObserveOperandMain.class',
                     'control/ObserveOperandMain.java', 'message-oracles.json',
                     'source-projection.json', 'reference.json', 'manifest.json', 'README.md']:
            with self.subTest(name=name):
                path = self.pack / name
                old = path.read_bytes()
                path.write_bytes(old + b' ')
                self.reject()
                path.write_bytes(old)

    def test_all_metadata_requires_canonical_lf(self):
        for name in ['manifest.json', 'harness.json', 'reference.json', 'source-projection.json']:
            path = self.pack / name
            old = path.read_bytes()
            path.write_bytes(old.replace(b'\n', b'\r\n'))
            self.reject()
            path.write_bytes(old)

    def test_tools_and_helper_are_pinned(self):
        for name in ref.TOOLS:
            path = self.root / 'tools' / name
            old = path.read_bytes()
            path.write_bytes(old + b'\n# changed\n')
            self.reject()
            path.write_bytes(old)

    def test_capture_does_not_replace_authority_on_late_drift(self):
        captured = self.capture()
        original = captured.blobs['source/app/OperandMain.java']
        (self.pack / 'source/app/OperandMain.java').write_bytes(b'wrong')
        with self.assertRaises(ValueError):
            captured.check()
        self.assertEqual(captured.blobs['source/app/OperandMain.java'], original)

    def test_late_tool_mutation_and_extra_file_refused(self):
        captured = self.capture()
        path = self.root / 'tools/fixture_source.py'
        path.write_bytes(path.read_bytes() + b'\n# drift\n')
        with self.assertRaises(ValueError):
            captured.check()
        captured = ref.capture(ref.PACK, ref.ROOT)
        extra = self.pack / 'source/new.class'
        extra.write_bytes(b'x')
        with self.assertRaises(ValueError):
            self.capture()
        captured.check()

    def test_wrong_oracles_are_explicit_independent_controls(self):
        c = self.capture()
        normal = ref.product_arguments(c)
        for case, position in [('wrong-bounds', 1), ('wrong-store', 19),
                               ('wrong-cast', 25), ('wrong-arithmetic', 33)]:
            wrong = ref.product_arguments(c, case)
            self.assertEqual(len(wrong), 34)
            self.assertNotEqual(wrong[position], normal[position])
            self.assertEqual([i for i in range(34) if wrong[i] != normal[i]], [position])
        with self.assertRaises(ValueError):
            ref.product_arguments(c, 'unknown')
        self.assertNotEqual(json.loads(c.blobs['message-oracles.json'])['zulu']['messages'][12],
                            json.loads(c.blobs['message-oracles.json'])['crabspot']['messages'][12])

    def test_reference_wrong_missing_duplicate_and_counter_rows_refused(self):
        rows = json.loads(self.capture().blobs['reference.json'])['runs']
        ref.validate_rows(rows)
        for changed in [rows[:-1], rows + rows[:1]]:
            with self.assertRaises(ValueError):
                ref.validate_rows(changed)
        for field, value in [('stdout', 'returned/184/42/6/8/8/247/29/1'),
                             ('stdout', ref.MAIN['normal']), ('exit_code', 1), ('vm', 'unknown')]:
            wrong = copy.deepcopy(rows)
            idx = next(i for i, row in enumerate(wrong) if row['case'] == 'wrong-bounds')
            wrong[idx][field] = value
            with self.assertRaises(ValueError):
                ref.validate_rows(wrong)

    def test_codec_wrong_exception_or_accepted_malformed_refused(self):
        rows = json.loads(self.capture().blobs['reference.json'])['codecs']
        ref.validate_codecs(rows)
        for change in [{'exit_code': 0}, {'stdout': 'guest executed'}, {'stderr': 'NoClassDefFoundError'}]:
            wrong = copy.deepcopy(rows)
            idx = next(i for i, row in enumerate(wrong) if row['token'] == 'T0')
            wrong[idx].update(change)
            with self.assertRaises(ValueError):
                ref.validate_codecs(wrong)
        wrong = copy.deepcopy(rows)
        next(row for row in wrong if row['token'] == 'T')['stdout'] = 'null'
        with self.assertRaises(ValueError):
            ref.validate_codecs(wrong)

    def test_pinned_build_wrong_inventory_or_bytes_refused(self):
        c = self.capture()
        original = {n: c.manifest['files']['classes/' + n] for n in ref.CLASSES}
        control = {'ObserveOperandMain.class': c.manifest['files']['controls/ObserveOperandMain.class']}
        self.assertTrue(ref.check_rebuilt(c, c.manifest['compiler'], c.manifest['compiler_sha256'], original, control))
        self.assertFalse(ref.check_rebuilt(c, 'javac 21.0.13', 'different', original, control))
        original['operand/Tag.class'] = 'bad'
        with self.assertRaises(ValueError):
            ref.check_rebuilt(c, c.manifest['compiler'], c.manifest['compiler_sha256'], original, control)
        with self.assertRaises(ValueError):
            ref.check_rebuilt(c, 'portable', 'different', {}, control)

    def test_fixed_stage_refuses_extra_missing_and_mutated_files(self):
        stage = self.root / 'stage'
        stage.mkdir()
        (stage / 'a').write_bytes(b'one')
        guard = ref.FixedTree(stage, ref.hashes(stage))
        (stage / 'extra').write_bytes(b'two')
        with self.assertRaises(ValueError):
            guard.check()
        (stage / 'extra').unlink()
        (stage / 'a').write_bytes(b'two')
        with self.assertRaises(ValueError):
            guard.check()
        (stage / 'a').unlink()
        with self.assertRaises(ValueError):
            guard.check()

    def test_between_subprocess_mutations_rejected_before_compile_or_guest(self):
        for name in ['source/app/OperandMain.java', 'classes/operand/Tag.class',
                     'controls/ObserveOperandMain.class', 'manifest.json']:
            work = self.root / ('stage-' + str(len(name)) + '-' + name.split('/')[0])
            def mutate(*args, **kwargs):
                (work / name).write_bytes(b'changed between external tools')
                return type('Result', (), {'returncode': 0, 'stdout': b'javac 21.0.7', 'stderr': b''})()
            with patch.object(ref, 'runtime_hashes', return_value={}), patch.object(ref.subprocess, 'run', side_effect=mutate) as process:
                with self.assertRaises(ValueError):
                    ref.prepare(self.pack, work, self.root / 'fake-jdk/bin/javac', self.root)
                self.assertEqual(process.call_count, 1)

    def test_javap_descriptor_does_not_replace_method_and_throws_is_supported(self):
        text = '  public static java.lang.Object loadA(java.lang.Object[], int);\n    descriptor: ([Ljava/lang/Object;I)Ljava/lang/Object;\n         7: aaload\n  public static void rethrow(java.lang.Throwable) throws java.lang.Throwable;\n    descriptor: (Ljava/lang/Throwable;)V\n         2: athrow\n'
        self.assertEqual(ref.parse_javap(text), {'loadA([Ljava/lang/Object;I)Ljava/lang/Object;': [[7, 'aaload']]})
        with self.assertRaises(ValueError):
            ref.parse_javap('       7: iaload\n')

    def test_git_style_lf_export_retains_exact_metadata_pins(self):
        exported=self.root/'lf-export'
        for name,path in ref.inventory(self.pack).items():
            data=path.read_bytes()
            if not name.endswith('.class'): data=data.replace(b'\r\n',b'\n')
            target=exported/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
        self.assertEqual(ref.capture(exported,self.root).blobs,self.capture().blobs)

    def test_supplied_capture_is_not_replaced_by_second_live_capture(self):
        captured=self.capture()
        old=captured.blobs['source/app/OperandMain.java']
        (self.pack/'source/app/OperandMain.java').write_bytes(b'changed')
        with patch.object(ref.subprocess,'run') as process:
            with self.assertRaises(ValueError):
                ref.prepare(self.pack,self.root/'stage-supplied',self.root/'jdk/bin/javac',self.root,captured=captured)
            process.assert_not_called()
        self.assertEqual(captured.blobs['source/app/OperandMain.java'],old)

    def scoped_report(self,scope):
        reference=json.loads(self.capture().blobs['reference.json'])
        names=ref.scope_runtimes(scope)
        versions={name:'openjdk version "21.0.13"'+(' OpenJ9' if name=='openj9' else ' HotSpot') for name in names}
        return {'runtime_scope':scope,'required_runtimes':list(names),'secondary_requested':scope=='dual',
                'observations':44*len(names),'codec_checks':12*len(names),
                'expected_observations':44*len(names),'expected_codec_checks':12*len(names),
                'versions':versions,'runtime_binaries':{name:{'version':versions[name],'path':'/jdk/'+name+'/bin/java','sha256':'0'*64} for name in names},
                'runs':[row for row in reference['runs'] if row['vm'] in names],
                'codecs':[row for row in reference['codecs'] if row['vm'] in names]}

    def test_explicit_primary_and_dual_scopes_are_exact(self):
        for scope in ['dual','primary-only']:
            report=self.scoped_report(scope)
            ref.validate_report(report,expected_scope=scope)
        primary=self.scoped_report('primary-only')
        with self.assertRaises(ValueError): ref.validate_rows(primary['runs'])
        with self.assertRaises(ValueError): ref.validate_codecs(primary['codecs'])
        with self.assertRaises(TypeError): ref.validate_report(primary)
        with self.assertRaises(ValueError): ref.validate_report(primary,expected_scope='unknown')

    def test_scope_fields_cannot_weaken_invocation_requirements(self):
        base=self.scoped_report('dual')
        for field,value in [('runtime_scope','primary-only'),('required_runtimes',['zulu']),
                            ('secondary_requested',False),('secondary_requested',1),
                            ('observations',44),('codec_checks',12),
                            ('expected_observations',44),('expected_codec_checks',12),
                            ('versions',{'openj9':base['versions']['openj9']}),
                            ('runtime_binaries',{'openj9':base['runtime_binaries']['openj9']})]:
            bad=copy.deepcopy(base);bad[field]=value
            with self.subTest(field=field,value=value), self.assertRaises(ValueError):
                ref.validate_report(bad,expected_scope='dual')

    def test_primary_scope_rejects_missing_duplicate_mixed_and_wrong_rows(self):
        base=self.scoped_report('primary-only')
        extra=self.scoped_report('dual')
        for field,value in [('runs',base['runs'][:-1]),('runs',base['runs']+base['runs'][:1]),
                            ('runs',extra['runs']),('codecs',base['codecs'][:-1]),
                            ('codecs',base['codecs']+base['codecs'][:1]),('codecs',extra['codecs']),
                            ('versions',{}),('required_runtimes',[]),('runtime_binaries',{})]:
            bad=copy.deepcopy(base);bad[field]=value
            with self.subTest(field=field), self.assertRaises(ValueError):
                ref.validate_report(bad,expected_scope='primary-only')
        bad=copy.deepcopy(base);bad['runs'][0]['stdout']='wrong'
        with self.assertRaises(ValueError): ref.validate_report(bad,expected_scope='primary-only')

    def test_primary_must_be_actual_hotspot_with_recorded_identity(self):
        base=self.scoped_report('primary-only')
        for replacement in ['openjdk version "21.0.13" OpenJ9','openjdk version "17.0.1" HotSpot','']:
            bad=copy.deepcopy(base);bad['versions']['zulu']=replacement;bad['runtime_binaries']['zulu']['version']=replacement
            with self.assertRaises(ValueError): ref.validate_report(bad,expected_scope='primary-only')
        for field,value in [('sha256','bad'),('version','different'),('path','')]:
            bad=copy.deepcopy(base);bad['runtime_binaries']['zulu'][field]=value
            with self.assertRaises(ValueError): ref.validate_report(bad,expected_scope='primary-only')

    def test_historical_authority_remains_dual_for_single_fresh_runs(self):
        p=self.pack/'reference.json';metadata=json.loads(p.read_bytes())
        metadata['runs']=[row for row in metadata['runs'] if row['vm']=='zulu']
        metadata['codecs']=[row for row in metadata['codecs'] if row['vm']=='zulu']
        p.write_text(json.dumps(metadata,indent=2)+'\n',encoding='utf8',newline='\n')
        h=self.pack/'harness.json';manifest=json.loads(h.read_bytes());manifest['files']['reference.json']=ref.digest(p.read_bytes());h.write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf8',newline='\n')
        self.reject()

    def test_python_default_stays_dual_and_contradictory_single_is_refused(self):
        fake=self.root/'jdk/bin/java'
        with patch.object(ref.subprocess,'run') as process:
            with self.assertRaises(ValueError): ref.run(self.pack,self.root,fake,fake,fake,None,self.root/'unrequested')
            with self.assertRaises(ValueError): ref.run(self.pack,self.root,fake,fake,fake,fake,self.root/'contradictory',single_primary=True)
            process.assert_not_called()
        self.assertFalse((self.root/'unrequested').exists())
        self.assertFalse((self.root/'contradictory').exists())

    def test_utf16_encoding_distinguishes_null_empty_and_surrogates(self):
        self.assertEqual(ref.encode(None), 'N')
        self.assertEqual(ref.encode(''), 'T')
        self.assertEqual(ref.encode('\x00\udfff\u20ac'), 'T0000dfff20ac')
        self.assertEqual(ref.encode('\uffff\ud800\udc00'), 'Tffffd800dc00')


if __name__ == '__main__':
    unittest.main()
