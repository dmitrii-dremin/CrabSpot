"""Replay the archived public Throwable application on independent Java 21 VMs.

This is fixture/reference tooling only. CrabSpot never invokes it to execute an
application. Checks are read-only; this tool cannot replace recorded evidence.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from fixture_source import read_java_source

ROOT = Path(__file__).resolve().parents[1]
PACK = ROOT / 'tests/fixtures/throwable-entry'
SOURCES = {'app/ThrowableMain.java', 'app/ThrowableRefilled.java',
           'app/ThrowableTraceAccess.java', 'app/ThrowableUncaught.java',
           'probe/ThrowableEntryGuest.java'}
CLASSES = {name.removesuffix('.java') + '.class' for name in SOURCES} | {
    'app/ThrowableMain$Forms.class', 'probe/ThrowableEntryGuest$UserFailure.class',
    'probe/ThrowableEntryGuest$ReplacementCtor.class', 'probe/ThrowableEntryGuest$VMFailingCtor.class',
}
INPUTS = ({'src/' + name for name in SOURCES} | {'classes/' + name for name in CLASSES}
          | {'controls/ObserveThrowableMain.java', 'projection.json'})
METADATA = {'manifest.json', 'harness.json', 'reference.json', 'README.md'}
TRANSFORMS = [
    {'from': 'ThrowableGuest', 'to': 'ThrowableEntryGuest', 'count': 1},
    {'from': 'Integer.MIN_VALUE', 'to': '(-2147483647 - 1)', 'count': 2},
    {'from': 'Long.MIN_VALUE', 'to': '(-9223372036854775807L - 1L)', 'count': 2},
]
FLAGS = ['--release', '21', '-g', '-encoding', 'UTF-8']
EXPECTED = {
    'main': '65535/8/4/1',
    'wrong': 'probe.ThrowableEntryGuest$UserFailure/app.ThrowableMain.monitorRefill/65535/0/true',
    'uncaught': 'java.lang.ArithmeticException/probe.ThrowableEntryGuest.idiv/7/0',
    'refilled': 'probe.ThrowableEntryGuest$UserFailure/probe.ThrowableEntryGuest.refill',
    'trace': '7/true',
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def tool_hash(path):
    raw = path.read_bytes().replace(b'\r\n', b'\n')
    if b'\r' in raw:
        raise ValueError('bare carriage return in tool source')
    return digest(raw)


def inventory(directory):
    paths = list(directory.rglob('*'))
    if any(path.is_symlink() for path in paths):
        raise ValueError('symlink in fixture inventory')
    return {path.relative_to(directory).as_posix(): path for path in paths if path.is_file()}


def validate_rows(rows):
    expected_keys = {(vm, stage, mode, case) for vm in ('zulu', 'openj9')
                     for stage in ('archived', 'rebuilt') for mode in ((), ('-Xint',))
                     for case in EXPECTED}
    seen = set()
    for row in rows:
        key = (row['vm'], row['input'], tuple(row['mode']), row['case'])
        if key not in expected_keys or key in seen or row['stdout'] != EXPECTED[row['case']]:
            raise ValueError('recorded reference observation differs')
        seen.add(key)
    if seen != expected_keys:
        raise ValueError('incomplete recorded reference matrix')


def capture(directory=PACK, root=ROOT):
    paths = inventory(directory)
    if set(paths) != INPUTS | METADATA:
        raise ValueError('unexpected public Throwable fixture inventory')
    # These very buffers supply compilation/execution; no post-check live rereads.
    blobs = {name: read_java_source(path) if name.endswith('.java') else path.read_bytes()
             for name, path in sorted(paths.items())}
    manifest = json.loads(blobs['manifest.json'])
    harness = json.loads(blobs['harness.json'])
    if manifest['schema'] != 1 or set(manifest['files']) != INPUTS or manifest['flags'] != FLAGS:
        raise ValueError('unexpected archive manifest schema or inputs')
    for name in INPUTS:
        if digest(blobs[name]) != manifest['files'][name]:
            raise ValueError('captured fixture bytes differ: ' + name)
    for name in ('manifest.json', 'reference.json'):
        if digest(blobs[name]) != harness['files'][name]:
            raise ValueError('recorded metadata changed: ' + name)
    tools = {name: tool_hash(root / 'tools' / name)
             for name in ('throwable_entry_reference.py', 'fixture_source.py')}
    if tools != harness['tools']:
        raise ValueError('reference tool provenance changed')
    validate_rows(json.loads(blobs['reference.json'])['runs'])

    historical = root / 'tests/fixtures/throwable'
    historical_manifest_raw = (historical / 'manifest.json').read_bytes()
    historical_manifest = json.loads(historical_manifest_raw)
    if digest(historical_manifest_raw) != harness['historical_manifest_sha256']:
        raise ValueError('historical manifest changed')
    source = read_java_source(historical / 'src/probe/ThrowableGuest.java')
    if digest(source) != historical_manifest['inputs']['src']['probe/ThrowableGuest.java']:
        raise ValueError('historical source changed')
    historical_classes = {n: digest(p.read_bytes()) for n, p in inventory(historical / 'classes').items()}
    if historical_classes != manifest['historical_inputs'] or historical_classes != historical_manifest['inputs']['classes']:
        raise ValueError('historical class inventory changed')
    projection = json.loads(blobs['projection.json'])
    if projection['transforms'] != TRANSFORMS or projection['historical_sha256'] != digest(source):
        raise ValueError('projection operations or historical hash differ')
    for operation in TRANSFORMS:
        old, new = operation['from'].encode(), operation['to'].encode()
        if source.count(old) != operation['count']:
            raise ValueError('projection operation count differs')
        source = source.replace(old, new)
    if source != blobs['src/probe/ThrowableEntryGuest.java'] or projection['projected_sha256'] != digest(source):
        raise ValueError('projected guest source differs')
    return blobs, manifest, tools


def process(command):
    result = subprocess.run([str(x) for x in command], capture_output=True, timeout=90)
    if result.returncode:
        raise ValueError(f'reference command failed ({result.returncode}): {command}\n{result.stderr!r}')
    return result.stdout.decode('utf8').strip(), result.stderr.decode('utf8').strip()


def tool_version(tool):
    output = '\n'.join(part for part in process([tool, '-version']) if part)
    if not re.match(r'(?:javac |(?:openjdk|java) version ")21(?:[."+\-]|$)', output):
        raise ValueError('Java 21 reference tool required: ' + output)
    return output.replace('\r\n', '\n')


def find_tool(name):
    java_home = os.environ.get('JAVA_HOME')
    result = shutil.which(name, path=str(Path(java_home) / 'bin') if java_home else None)
    if result is None:
        raise ValueError('missing Java 21 reference tool: ' + name)
    return Path(result)


def check_rebuilt(manifest, compiler_version, compiler_hash, rebuilt, controls):
    if set(rebuilt) != CLASSES or set(controls) != {'ObserveThrowableMain.class'}:
        raise ValueError('unexpected reference compiler output inventory')
    pinned = (compiler_version == '\n'.join(filter(None, manifest['compiler'])).replace('\r\n', '\n')
              and compiler_hash == manifest['compiler_sha256'])
    original = {name: manifest['files']['classes/' + name] for name in CLASSES}
    if pinned and (rebuilt != original or controls != manifest['compiled_controls']):
        raise ValueError('pinned compiler did not reproduce archived bytes')
    return pinned


def hashes(directory):
    return {name: digest(path.read_bytes()) for name, path in inventory(directory).items()}


def verify_staged_inputs(work, blobs):
    for group in ('src', 'classes', 'controls'):
        expected = {name.removeprefix(group + '/'): digest(data)
                    for name, data in blobs.items() if name.startswith(group + '/')}
        if hashes(work / group) != expected:
            raise ValueError('staged captured input changed: ' + group)


def prepare(directory, work, javac, root=ROOT):
    captured = capture(directory, root)
    blobs, manifest, _ = captured
    for name in sorted(INPUTS):
        if name == 'projection.json':
            continue
        destination = work / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(blobs[name])
    verify_staged_inputs(work, blobs)
    compiler = tool_version(javac)
    compiler_hash = digest(Path(javac).read_bytes())
    original, rebuilt, controls = work / 'classes', work / 'rebuilt', work / 'control-classes'
    verify_staged_inputs(work, blobs)
    process([javac, *FLAGS, '-d', rebuilt, *[work / 'src' / name for name in sorted(SOURCES)]])
    verify_staged_inputs(work, blobs)
    process([javac, *FLAGS, '-cp', original, '-d', controls, work / 'controls/ObserveThrowableMain.java'])
    verify_staged_inputs(work, blobs)
    pinned = check_rebuilt(manifest, compiler, compiler_hash, hashes(rebuilt), hashes(controls))
    return captured, original, rebuilt, controls, pinned


def check(directory=PACK, other=None, root=ROOT):
    java, javac = find_tool('java'), find_tool('javac')
    scratch = root / 'target/fixture-tool'
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='throwable-entry-', dir=scratch) as tmp:
        captured, original, rebuilt, controls, pinned = prepare(directory, Path(tmp), javac, root)
        before = {str(path): hashes(path) for path in (original, rebuilt, controls, Path(tmp) / 'src', Path(tmp) / 'controls')}
        runtimes = [('primary', java, [])]
        if other:
            runtimes.append(('openj9', Path(other), ['-Xshareclasses:none']))
        rows, versions = [], {}
        for vendor, vm, options in runtimes:
            versions[vendor] = tool_version(vm)
            if vendor == 'openj9' and 'OpenJ9' not in versions[vendor]:
                raise ValueError('--compare-openj9 requires an OpenJ9 runtime')
            for stage, classes in [('archived', original), ('rebuilt', rebuilt)]:
                for mode in ([], ['-Xint']):
                    for case, expected in EXPECTED.items():
                        stdout, stderr = process([vm, *options, *mode, '-Xverify:all', '-cp',
                                                  str(controls) + os.pathsep + str(classes),
                                                  'ObserveThrowableMain', case])
                        if stdout != expected:
                            raise ValueError(f'{vendor}/{stage}/{mode}/{case}: expected {expected}, got {stdout}')
                        rows.append({'vm': vendor, 'input': stage, 'mode': mode,
                                     'case': case, 'stdout': stdout, 'stderr': stderr})
        for path, expected in before.items():
            if hashes(Path(path)) != expected:
                raise ValueError('staged reference input changed during execution')
        if capture(directory, root) != captured:
            raise ValueError('live reference inputs changed during execution')
        return {'observations': len(rows), 'compiler_exact_rebuild': pinned,
                'versions': versions, 'runs': rows, 'recording_written': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='check the immutable archive (the only mode)')
    parser.add_argument('--compare-openj9', type=Path)
    args = parser.parse_args()
    result = check(other=args.compare_openj9)
    print(json.dumps({key: result[key] for key in ('observations', 'compiler_exact_rebuild', 'recording_written')}, indent=2))
