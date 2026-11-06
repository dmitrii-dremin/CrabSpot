import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import throwable_entry_reference as ref


class ThrowableEntryReferenceTests(unittest.TestCase):
    def setUp(self):
        scratch = ref.ROOT / 'target/fixture-tool'
        scratch.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix='throwable-entry-test-', dir=scratch)
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.pack = self.root / 'tests/fixtures/throwable-entry'
        shutil.copytree(ref.PACK, self.pack)
        shutil.copytree(ref.ROOT / 'tests/fixtures/throwable', self.root / 'tests/fixtures/throwable')
        (self.root / 'tools').mkdir()
        for name in ('throwable_entry_reference.py', 'fixture_source.py'):
            shutil.copyfile(ref.ROOT / 'tools' / name, self.root / 'tools' / name)

    def capture(self):
        return ref.capture(self.pack, self.root)

    def rewrite_json(self, name, value):
        raw = (json.dumps(value, indent=2) + '\n').encode()
        (self.pack / name).write_bytes(raw)
        return ref.digest(raw)

    def refresh_manifest_pin(self):
        harness = json.loads((self.pack / 'harness.json').read_bytes())
        harness['files']['manifest.json'] = ref.digest((self.pack / 'manifest.json').read_bytes())
        self.rewrite_json('harness.json', harness)

    def test_pristine_archive_has_only_expected_guest_and_control_inputs(self):
        blobs, manifest, tools = self.capture()
        self.assertEqual(set(blobs), ref.INPUTS | ref.METADATA)
        self.assertEqual(len(ref.CLASSES), 9)
        self.assertEqual(manifest['flags'], ref.FLAGS)
        self.assertEqual(set(tools), {'throwable_entry_reference.py', 'fixture_source.py'})
        ref.validate_rows(json.loads(blobs['reference.json'])['runs'])

    def test_canonical_lf_checkout_preserves_exact_provenance(self):
        # .gitattributes exports text as LF, even when an author's working copy
        # contains CRLF. Reproduce that boundary before checking recorded hashes.
        for name in ref.INPUTS | ref.METADATA:
            if not name.endswith('.class'):
                path = self.pack / name
                path.write_bytes(path.read_bytes().replace(b'\r\n', b'\n'))
        blobs, manifest, _ = self.capture()
        self.assertEqual(ref.digest(blobs['projection.json']),
                         manifest['files']['projection.json'])

    def test_projection_bytes_remain_exact_after_checkout(self):
        path = self.pack / 'projection.json'
        original = path.read_bytes()
        self.assertNotIn(b'\r', original)
        for changed in (original.replace(b'\n', b'\r\n'), original + b' '):
            with self.subTest(changed=ref.digest(changed)):
                self.assertEqual(json.loads(changed), json.loads(original))
                path.write_bytes(changed)
                with self.assertRaisesRegex(ValueError,
                                            'captured fixture bytes differ: projection.json'):
                    self.capture()
                path.write_bytes(original)

    def test_extra_class_source_or_observer_output_fails_before_tools(self):
        for name in ('classes/Injected.class', 'src/Injected.java', 'controls/ObserveThrowableMain.class'):
            with self.subTest(name=name):
                path = self.pack / name
                path.write_bytes(b'extra')
                with patch.object(ref, 'process') as process:
                    with self.assertRaisesRegex(ValueError, 'inventory'):
                        ref.prepare(self.pack, self.root / 'scratch', Path('no-javac'), self.root)
                    process.assert_not_called()
                path.unlink()

    def test_missing_class_input_is_rejected(self):
        (self.pack / 'classes/app/ThrowableMain.class').unlink()
        with self.assertRaisesRegex(ValueError, 'inventory'):
            self.capture()

    def test_source_class_and_observer_mutations_fail_before_tools(self):
        for name in ('src/app/ThrowableMain.java', 'classes/app/ThrowableMain.class',
                     'controls/ObserveThrowableMain.java'):
            with self.subTest(name=name):
                path = self.pack / name
                original = path.read_bytes()
                path.write_bytes(original + b'changed')
                with patch.object(ref, 'process') as process:
                    with self.assertRaisesRegex(ValueError, 'captured fixture bytes'):
                        ref.prepare(self.pack, self.root / 'scratch', Path('no-javac'), self.root)
                    process.assert_not_called()
                path.write_bytes(original)

    def test_java_crlf_is_not_silently_canonicalized(self):
        path = self.pack / 'src/app/ThrowableMain.java'
        path.write_bytes(path.read_bytes().replace(b'\n', b'\r\n'))
        with self.assertRaisesRegex(ValueError, 'LF line endings'):
            self.capture()

    def test_exact_consumed_blob_is_authenticated(self):
        target = self.pack / 'classes/app/ThrowableMain.class'
        real_read = Path.read_bytes

        def read(path):
            value = real_read(path)
            return value + b'changed' if path == target else value

        with patch.object(Path, 'read_bytes', read):
            with self.assertRaisesRegex(ValueError, 'captured fixture bytes'):
                self.capture()

    def test_captured_buffers_do_not_follow_live_mutations(self):
        blobs, _, _ = self.capture()
        path = self.pack / 'src/app/ThrowableMain.java'
        path.write_bytes(b'changed after capture')
        self.assertIn(b'public final class ThrowableMain', blobs['src/app/ThrowableMain.java'])
        with self.assertRaises(ValueError):
            self.capture()

    def test_subprocess_cannot_mutate_staged_inputs_before_the_first_vm(self):
        javac = self.root / 'fake-javac'
        javac.write_bytes(b'fake executable')
        for index, name in enumerate(('classes/app/ThrowableMain.class',
                                      'src/app/ThrowableMain.java',
                                      'controls/ObserveThrowableMain.java')):
            with self.subTest(name=name):
                work = self.root / ('staged-' + str(index))

                def mutate(command):
                    if command[1] == '-version':
                        return 'javac 21.0.7', ''
                    target = work / name
                    target.write_bytes(target.read_bytes() + b'changed during compiler subprocess')
                    return '', ''

                with patch.object(ref, 'process', side_effect=mutate) as process:
                    with self.assertRaisesRegex(ValueError, 'staged captured input changed'):
                        ref.prepare(self.pack, work, javac, self.root)
                    self.assertEqual(process.call_count, 2)
                    self.assertFalse(any('ObserveThrowableMain' in call.args[0] for call in process.call_args_list))

    def test_self_consistent_extra_manifest_input_cannot_expand_inventory(self):
        manifest = json.loads((self.pack / 'manifest.json').read_bytes())
        manifest['files']['classes/Injected.class'] = ref.digest(b'injected')
        (self.pack / 'classes/Injected.class').write_bytes(b'injected')
        self.rewrite_json('manifest.json', manifest)
        self.refresh_manifest_pin()
        with self.assertRaisesRegex(ValueError, 'inventory'):
            self.capture()

    def test_projection_cannot_authorize_an_additional_rewrite(self):
        projection = json.loads((self.pack / 'projection.json').read_bytes())
        projection['transforms'].append({'from': 'return', 'to': 'throw', 'count': 1})
        checksum = self.rewrite_json('projection.json', projection)
        manifest = json.loads((self.pack / 'manifest.json').read_bytes())
        manifest['files']['projection.json'] = checksum
        self.rewrite_json('manifest.json', manifest)
        self.refresh_manifest_pin()
        with self.assertRaisesRegex(ValueError, 'projection operations'):
            self.capture()

    def test_wrong_claimed_projected_hash_is_rejected(self):
        projection = json.loads((self.pack / 'projection.json').read_bytes())
        projection['projected_sha256'] = '0' * 64
        checksum = self.rewrite_json('projection.json', projection)
        manifest = json.loads((self.pack / 'manifest.json').read_bytes())
        manifest['files']['projection.json'] = checksum
        self.rewrite_json('manifest.json', manifest)
        self.refresh_manifest_pin()
        with self.assertRaisesRegex(ValueError, 'projected guest source'):
            self.capture()

    def test_reference_rows_preserve_guard_identity_counter_and_trace_head(self):
        original = json.loads((self.pack / 'reference.json').read_bytes())
        for case, changed in [('main', '65534/8/4/1'), ('wrong', ref.EXPECTED['wrong'].replace('true', 'false')),
                              ('uncaught', ref.EXPECTED['uncaught'].replace('idiv', 'refill'))]:
            with self.subTest(case=case):
                rows = json.loads(json.dumps(original['runs']))
                next(row for row in rows if row['case'] == case)['stdout'] = changed
                with self.assertRaisesRegex(ValueError, 'observation differs'):
                    ref.validate_rows(rows)
        with self.assertRaisesRegex(ValueError, 'incomplete'):
            ref.validate_rows(original['runs'][:-1])
        with self.assertRaisesRegex(ValueError, 'observation differs'):
            ref.validate_rows(original['runs'] + [original['runs'][0]])

    def test_reference_bytes_and_historical_manifest_are_pinned(self):
        reference_path = self.pack / 'reference.json'
        original = reference_path.read_bytes()
        reference_path.write_bytes(original + b' ')
        with self.assertRaisesRegex(ValueError, 'recorded metadata changed'):
            self.capture()
        reference_path.write_bytes(original)
        historical = self.root / 'tests/fixtures/throwable/manifest.json'
        historical.write_bytes(historical.read_bytes() + b' ')
        with self.assertRaisesRegex(ValueError, 'historical manifest changed'):
            self.capture()

    def test_tool_changes_require_new_reviewed_provenance(self):
        path = self.root / 'tools/throwable_entry_reference.py'
        path.write_bytes(path.read_bytes() + b'\n# changed\n')
        with self.assertRaisesRegex(ValueError, 'tool provenance'):
            self.capture()

    def test_compiler_identity_controls_byte_reproducibility_but_always_checks_inventory(self):
        _, manifest, _ = self.capture()
        original = {name: manifest['files']['classes/' + name] for name in ref.CLASSES}
        compiler = '\n'.join(filter(None, manifest['compiler']))
        altered = original | {'app/ThrowableMain.class': 'different'}
        self.assertTrue(ref.check_rebuilt(manifest, compiler, manifest['compiler_sha256'],
                                         original, manifest['compiled_controls']))
        with self.assertRaisesRegex(ValueError, 'pinned compiler'):
            ref.check_rebuilt(manifest, compiler, manifest['compiler_sha256'], altered, manifest['compiled_controls'])
        self.assertFalse(ref.check_rebuilt(manifest, compiler, 'other executable', altered,
                                          {'ObserveThrowableMain.class': 'different'}))
        with self.assertRaisesRegex(ValueError, 'inventory'):
            ref.check_rebuilt(manifest, compiler, 'other executable', altered | {'Injected.class': 'x'},
                              manifest['compiled_controls'])

    @unittest.skipUnless(os.environ.get('JAVA_HOME'), 'explicit Java 21 reference JDK needed')
    def test_real_archived_and_rebuilt_reference_runs_are_read_only(self):
        before = ref.hashes(self.pack)
        result = ref.check(self.pack, root=self.root)
        self.assertEqual(result['observations'], 20)
        self.assertFalse(result['recording_written'])
        self.assertEqual(ref.hashes(self.pack), before)


if __name__ == '__main__':
    unittest.main()
