"""No-JDK provenance regressions for original fixture and observation ownership."""
from pathlib import Path
import copy, importlib.util, json, os, shutil, subprocess, sys, tempfile, unittest
from unittest import mock
from contextlib import redirect_stdout
import io
TOOL = Path(__file__).resolve().with_name('throwable_reference.py')
spec = importlib.util.spec_from_file_location('throwable_reference', TOOL)
ref = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ref)
PACK = ref.PACK
JAVA_NAME = 'java.exe' if os.name == 'nt' else 'java'
JAVAC_NAME = 'javac.exe' if os.name == 'nt' else 'javac'
WORK = Path(os.environ.get('CRABSPOT_THROWABLE_TEST_WORK', ref.ROOT / 'target/fixture-tool/throwable-provenance'))
WORK.mkdir(parents=True, exist_ok=True)

class Provenance(unittest.TestCase):

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='case-', dir=WORK)
        self.pack = Path(self.temp.name) / 'fixture'
        self.pack.mkdir()
        for group in ['src', 'controls', 'classes', 'observations', 'filters']:
            shutil.copytree(PACK / group, self.pack / group)
        for name in ['manifest.json', 'portable.json']:
            shutil.copyfile(PACK / name, self.pack / name)
        self.manifest = json.loads((self.pack / 'manifest.json').read_text(encoding='utf-8'))

    def tearDown(self):
        self.temp.cleanup()

    def test_pristine_archive_contains_only_guest_classes(self):
        ref.verify_inputs(self.pack, self.manifest, TOOL)
        self.assertEqual(set(ref.inventory(self.pack / 'classes')), ref.CLASS_NAMES)

    def test_source_comment_mutation_is_detected(self):
        p = self.pack / 'src/probe/ThrowableGuest.java'
        p.write_bytes(p.read_bytes() + b'\n// changed\n')
        with self.assertRaisesRegex(ref.EvidenceError, 'input hashes|inventory'):
            ref.verify_inputs(self.pack, self.manifest)

    def test_original_class_byte_mutation_is_detected(self):
        p = self.pack / 'classes/probe/ThrowableGuest.class'
        b = bytearray(p.read_bytes())
        b[-1] ^= 1
        p.write_bytes(b)
        with self.assertRaisesRegex(ref.EvidenceError, 'input hashes|inventory'):
            ref.verify_inputs(self.pack, self.manifest)

    def test_observer_mutation_is_detected(self):
        p = self.pack / 'controls/probe/ObserveThrowable.java'
        p.write_bytes(p.read_bytes() + b'\n// changed observer\n')
        with self.assertRaisesRegex(ref.EvidenceError, 'input hashes|inventory'):
            ref.verify_inputs(self.pack, self.manifest)

    def test_archiving_observer_as_guest_is_detected(self):
        (self.pack / 'classes/ObserveThrowable.class').write_bytes(b'not guest')
        with self.assertRaisesRegex(ref.EvidenceError, 'input hashes|inventory'):
            ref.verify_inputs(self.pack, self.manifest)

    def test_recorded_observation_mutation_is_detected(self):
        p = self.pack / 'observations/zulu.json'
        p.write_bytes(p.read_bytes() + b' ')
        with self.assertRaisesRegex(ref.EvidenceError, 'observation hash'):
            ref.verify_inputs(self.pack, self.manifest)

    def test_missing_class_input_is_rejected(self):
        (self.pack / 'classes/probe/ThrowableGuest.class').unlink()
        with self.assertRaisesRegex(ref.EvidenceError, 'input hashes|inventory'):
            ref.verify_inputs(self.pack, self.manifest)

    def test_portable_comparator_rejects_reused_vm_identity_and_wrong_fault_effects(self):
        rows = json.loads((self.pack / 'observations/zulu.json').read_text(encoding='utf-8'))
        for key, value in [('fresh', 'false'), ('after', '1002'), ('cause', 'java.lang.RuntimeException')]:
            changed = copy.deepcopy(rows)
            next((r for r in changed if r['case'] == 'idiv-0'))[key] = value
            with self.assertRaises(ref.EvidenceError):
                ref.portable(changed)

    def test_real_check_command_rejects_source_before_attempting_java(self):
        p = self.pack / 'src/probe/ThrowableGuest.java'
        p.write_bytes(p.read_bytes() + b'\n// changed\n')
        self.cli_rejects()

    def test_real_check_command_rejects_class_before_attempting_java(self):
        p = self.pack / 'classes/probe/ThrowableGuest.class'
        p.write_bytes(p.read_bytes() + b'x')
        self.cli_rejects()

    def cli_rejects(self):
        r = subprocess.run([sys.executable, str(TOOL), '--check', '--fixtures', str(self.pack), '--java', 'missing-java.exe', '--javac', 'missing-javac.exe', '--compare-openj9', 'missing-openj9.exe', '--work', str(Path(self.temp.name) / 'work')], capture_output=True, timeout=15)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn(b'input hashes differ', r.stderr)
        self.assertNotIn(b'FileNotFoundError', r.stderr)

    def test_java_crlf_is_rejected_before_subprocess(self):
        p = self.pack / 'src/probe/ThrowableGuest.java'
        p.write_bytes(p.read_bytes().replace(b'\n', b'\r\n'))
        with mock.patch.object(ref.subprocess, 'run') as process:
            with self.assertRaisesRegex(ref.EvidenceError, 'LF line endings'):
                ref.main(['--check', '--fixtures', str(self.pack), '--java', 'missing.exe', '--javac', 'missing.exe', '--work', str(Path(self.temp.name) / 'work')])
            process.assert_not_called()

    def test_python_crlf_tool_hash_is_canonical_and_bare_cr_rejected(self):
        p = Path(self.temp.name) / 'tool.py'
        data = (TOOL).read_bytes().replace(b'\r\n', b'\n')
        p.write_bytes(data.replace(b'\n', b'\r\n'))
        self.assertEqual(ref.tool_digest(p), ref.tool_digest(TOOL))
        p.write_bytes(data + b'\r')
        with self.assertRaisesRegex(ref.EvidenceError, 'bare carriage'):
            ref.tool_digest(p)

    def test_unpinned_compiler_checks_inventory_not_rebuilt_hashes(self):
        recorded = self.manifest['inputs']['classes']
        changed = dict(recorded)
        changed[next(iter(changed))] = '0' * 64
        self.assertTrue(ref.requires_exact_rebuild(self.manifest['compiler'], self.manifest['compiler_sha256'], self.manifest))
        self.assertFalse(ref.requires_exact_rebuild('javac 21.0.99', 'different', self.manifest))
        self.assertFalse(ref.requires_exact_rebuild(self.manifest['compiler'], 'different platform binary', self.manifest))
        ref.check_rebuild(recorded, changed, False)
        with self.assertRaisesRegex(ref.EvidenceError, 'pinned compiler'):
            ref.check_rebuild(recorded, changed, True)
        changed['NotGuest.class'] = '0' * 64
        with self.assertRaisesRegex(ref.EvidenceError, 'inventory'):
            ref.check_rebuild(recorded, changed, False)

    def test_ordinary_record_requires_current_tool_pin_before_any_process(self):
        self.manifest['tool_sha256'] = '0' * 64
        (self.pack / 'manifest.json').write_text(json.dumps(self.manifest), encoding='utf-8')
        with mock.patch.object(ref.subprocess, 'run') as process:
            with self.assertRaisesRegex(ref.EvidenceError, 'tool hash'):
                ref.main(['--fixtures', str(self.pack), '--java', 'missing.exe', '--javac', 'missing.exe', '--compare-openj9', 'missing.exe', '--work', str(Path(self.temp.name) / 'work')])
            process.assert_not_called()

    @unittest.skipUnless(os.environ.get('JAVA_HOME') and (Path(os.environ['JAVA_HOME']) / 'bin' / JAVAC_NAME).is_file(), 'explicit Java21 reference JDK needed')
    def test_midrun_live_source_class_and_extra_source_never_supply_subprocess_inputs(self):
        home = Path(os.environ['JAVA_HOME'])
        work = Path(self.temp.name) / 'work'
        original = ref.capture_inputs(self.pack)
        seen = {'injected': False, 'sources': False, 'original': False}
        real_run = subprocess.run

        def process(argv, **kwargs):
            if not seen['injected']:
                seen['injected'] = True
                (self.pack / 'src/probe/ThrowableGuest.java').write_bytes(b'not valid Java')
                (self.pack / 'src/Evil.java').write_bytes(b'not valid Java')
                (self.pack / 'classes/probe/ThrowableGuest.class').write_bytes(b'not valid class')
            sources = [Path(a) for a in argv if str(a).endswith('.java')]
            for path in sources:
                group = 'controls' if '/controls/' in path.as_posix() else 'src'
                name = path.relative_to(path.parents[1] if path.parent.name == 'probe' else path.parent).as_posix()
                self.assertIn(name, original[group])
                self.assertEqual(path.read_bytes(), original[group][name])
                self.assertNotIn('Evil', path.name)
                self.assertTrue(path.is_relative_to(work))
                seen['sources'] = True
            if 'probe.ObserveThrowable' in argv and any(('original' in str(a) for a in argv)):
                cp = Path(str(argv[argv.index('-cp') + 1]).split(os.pathsep)[-1])
                for name, data in original['classes'].items():
                    self.assertEqual((cp / name).read_bytes(), data)
                seen['original'] = True
            return real_run(argv, **kwargs)
        with mock.patch.object(ref.subprocess, 'run', side_effect=process), redirect_stdout(io.StringIO()):
            with self.assertRaisesRegex(ref.EvidenceError, 'changed during replay'):
                ref.main(['--check', '--portable-primary', '--fixtures', str(self.pack), '--java', str(home / 'bin' / JAVA_NAME), '--javac', str(home / 'bin' / JAVAC_NAME), '--work', str(work)])
        self.assertTrue(all(seen.values()))

    @unittest.skipUnless(os.environ.get('JAVA_HOME') and (Path(os.environ['JAVA_HOME']) / 'bin' / JAVAC_NAME).is_file() and (ref.digest((Path(os.environ['JAVA_HOME']) / 'bin' / JAVAC_NAME).read_bytes()) == json.loads((PACK / 'manifest.json').read_text(encoding='utf-8'))['compiler_sha256']), 'pinned reference compiler needed')
    def test_repeat_record_cannot_replace_self_consistent_changed_historical_rows(self):
        home = Path(os.environ['JAVA_HOME'])
        p = self.pack / 'observations/zulu.json'
        rows = json.loads(p.read_text(encoding='utf-8'))
        rows[0]['writes'] = '999'
        p.write_text(json.dumps(rows), encoding='utf-8')
        self.manifest['outputs']['observations/zulu.json'] = ref.digest(p.read_bytes())
        (self.pack / 'manifest.json').write_text(json.dumps(self.manifest), encoding='utf-8')
        before = {n: (self.pack / n).read_bytes() for n in ['manifest.json', *ref.OUTPUTS]}
        with redirect_stdout(io.StringIO()):
            with self.assertRaisesRegex(ref.EvidenceError, 'historical Throwable'):
                ref.main(['--fixtures', str(self.pack), '--java', str(home / 'bin' / JAVA_NAME), '--javac', str(home / 'bin' / JAVAC_NAME), '--compare-openj9', 'never-reached.exe', '--work', str(Path(self.temp.name) / 'work')])
        self.assertEqual(before, {n: (self.pack / n).read_bytes() for n in before})

    @unittest.skipUnless(os.environ.get('JAVA_HOME') and (Path(os.environ['JAVA_HOME']) / 'bin' / JAVAC_NAME).is_file(), 'explicit Java21 reference JDK needed')
    def test_default_tools_and_unique_workspace_temp_entry(self):
        checkout = Path(self.temp.name) / 'checkout'
        before = (self.pack / 'manifest.json').read_bytes()
        with mock.patch.object(ref, 'ROOT', checkout), mock.patch.object(ref, 'PACK', self.pack), redirect_stdout(io.StringIO()):
            result = ref.main(['--check', '--portable-primary'])
        self.assertFalse(result['recording_written'])
        self.assertEqual((self.pack / 'manifest.json').read_bytes(), before)
        self.assertEqual(list((checkout / 'target/fixture-tool').iterdir()), [])
    def test_unpinned_run_diagnostics_can_differ_without_semantic_envelope_drift(self):
        rows=json.loads((self.pack/'observations/zulu.json').read_text(encoding='utf-8'))
        changed=copy.deepcopy(rows)
        value=next(r for r in changed if r['case']=='idiv-0')
        top=value['trace'].split(',')[0].split('#')
        top[2]='DifferentSource.java';top[3]='999'
        value['trace']='#'.join(top)+',different.Deep#method#Deep.java#123#false'
        value['traceSize']='22';value['message16']='006f006b'
        filters=json.loads((self.pack/'filters/zulu.json').read_text(encoding='utf-8'))
        changed_filters=copy.deepcopy(filters)
        next(r for r in changed_filters if r['case']=='covariant-override')['lines']='different line diagnostics'
        self.assertEqual(ref.portable(rows),ref.portable(changed))
        self.assertEqual(ref.filter_portable(filters),ref.filter_portable(changed_filters))
        ref.compare_run_diagnostics([rows,changed],[filters,changed_filters],False)
        with self.assertRaisesRegex(ref.EvidenceError,'strict vendor'):
            ref.compare_run_diagnostics([rows,changed],[filters,changed_filters],True)
        value['before']='999'
        with self.assertRaises(ref.EvidenceError):ref.portable(changed)
        value['before']='1001';value['trace']=value['trace'].replace('#idiv#','#wrongMethod#',1)
        with self.assertRaises(ref.EvidenceError):ref.portable(changed)

if __name__ == '__main__':
    unittest.main()
