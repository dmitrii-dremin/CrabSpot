"""Original Java21 Throwable evidence; application execution never delegates to a JDK.

All archived bytes are captured before any subprocess. Normal repeat recording is
read-only; reviewed source/tool updates must preserve existing historical rows.
"""
from pathlib import Path
from contextlib import nullcontext
import argparse, hashlib, json, os, re, shutil, subprocess, tempfile
from fixture_source import read_java_source
TOOL = Path(__file__).resolve()
ROOT = next((p for p in TOOL.parents if (p / 'Cargo.toml').is_file()), TOOL.parents[1])
LOCAL_PACK = TOOL.parents[1]
PACK = LOCAL_PACK if (LOCAL_PACK / 'manifest.json').is_file() else ROOT / 'tests/fixtures/throwable'
FLAGS = ['--release', '21', '-g', '-encoding', 'UTF-8']
GUEST = {'probe/ThrowableGuest.java'}
CONTROLS = {'probe/ObserveThrowable.java', 'PlatformIdentity.java', 'TraceFilterObserver.java'}
CLASS_NAMES = {'probe/ThrowableGuest.class', 'probe/ThrowableGuest$UserFailure.class', 'probe/ThrowableGuest$ReplacementCtor.class', 'probe/ThrowableGuest$VMFailingCtor.class'}
OUTPUTS = {'portable.json', 'observations/zulu.json', 'observations/openj9.json', 'filters/zulu.json', 'filters/openj9.json'}

class EvidenceError(ValueError):
    pass

def digest(data):
    return hashlib.sha256(data).hexdigest()

def tool_digest(path):
    data = path.read_bytes().replace(b'\r\n', b'\n')
    if b'\r' in data:
        raise EvidenceError('bare carriage return in tool source')
    return digest(data)

def file_names(root):
    result = []
    for p in sorted(root.rglob('*')):
        if p.is_symlink():
            raise EvidenceError('symlink in reference inputs')
        if p.is_file():
            result.append(p.relative_to(root).as_posix())
    return result

def inventory(root):
    return {n: digest((root / n).read_bytes()) for n in file_names(root)}

def capture_inputs(pack):
    blobs = {}
    for group, expected in [('src', GUEST), ('controls', CONTROLS), ('classes', CLASS_NAMES)]:
        names = set(file_names(pack / group))
        if names != expected and (not (group == 'classes' and (not names))):
            raise EvidenceError('unexpected ' + group + ' inventory')
        blobs[group] = {}
        for name in sorted(names):
            path = pack / group / name
            try:
                data = read_java_source(path) if group != 'classes' else path.read_bytes()
            except ValueError as e:
                raise EvidenceError(str(e)) from e
            if group != 'classes':
                data.decode('utf-8')
            blobs[group][name] = data
    return blobs

def blob_hashes(blobs):
    return {g: {n: digest(b) for n, b in rows.items()} for g, rows in blobs.items()}

def inputs(pack):
    return blob_hashes(capture_inputs(pack))

def load_history(pack):
    path = pack / 'manifest.json'
    if not path.exists():
        return None
    raw = path.read_bytes()
    m = json.loads(raw)
    if set(m['outputs']) != OUTPUTS:
        raise EvidenceError('unexpected historical observation inventory')
    data = {name: (pack / name).read_bytes() for name in sorted(OUTPUTS)}
    for name, value in data.items():
        if digest(value) != m['outputs'][name]:
            raise EvidenceError('recorded observation hash differs: ' + name)
    return {'manifest': m, 'manifest_sha256': digest(raw), 'outputs': {n: json.loads(b) for n, b in data.items()}}

def validate_capture(blobs, history, tool_hash, reviewed=False, helper_hash=None):
    if history is None:
        return
    m = history['manifest']
    if blob_hashes(blobs) != m['inputs'] and (not reviewed):
        raise EvidenceError('reference source/class input hashes differ')
    if tool_hash != m['tool_sha256'] and (not reviewed):
        raise EvidenceError('reference tool hash differs')
    helper_hash = helper_hash or tool_digest(Path(read_java_source.__code__.co_filename))
    if m.get('fixture_source_sha256', helper_hash) != helper_hash and (not reviewed):
        raise EvidenceError('shared fixture source helper hash differs')

def verify_inputs(pack, manifest, tool=None):
    if inputs(pack) != manifest['inputs']:
        raise EvidenceError('reference source/class input hashes differ')
    history = load_history(pack)
    if history is None:
        raise EvidenceError('missing recording')
    if tool is not None and tool_digest(tool) != manifest['tool_sha256']:
        raise EvidenceError('reference tool hash differs')

def stage_inputs(blobs, scratch):
    for group, rows in blobs.items():
        dest = scratch / ('original' if group == 'classes' else group)
        dest.mkdir(parents=True, exist_ok=True)
        for name, data in rows.items():
            path = dest / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)

def requires_exact_rebuild(version, compiler_hash, manifest):
    return version == manifest['compiler'] and compiler_hash == manifest['compiler_sha256']

def check_rebuild(original, rebuilt, exact):
    if set(original) != CLASS_NAMES or set(rebuilt) != CLASS_NAMES:
        raise EvidenceError('guest class inventory differs')
    if exact and original != rebuilt:
        raise EvidenceError('pinned compiler did not reproduce guest bytes')

def parse(text):
    rows = []
    for line in text.splitlines():
        cols = line.split('\t')
        row = {'case': cols[0]}
        row.update((x.split('=', 1) for x in cols[1:]))
        rows.append(row)
    return rows
VERSIONS = {'javac': 'javac 21.0.7',
 'zulu': 'openjdk version "21.0.7" 2025-04-15 LTS\n'
         'OpenJDK Runtime Environment Zulu21.42+19-CA (build 21.0.7+6-LTS)\n'
         'OpenJDK 64-Bit Server VM Zulu21.42+19-CA (build 21.0.7+6-LTS, mixed mode, sharing)',
 'openj9': 'openjdk version "21.0.13.0.3" 2026-10-24 LTS\n'
           'IBM Semeru Runtime Open Edition 21.0.13.0 (build 21.0.13.0.3+1-LTS)\n'
           'Eclipse OpenJ9 VM 21.0.13.0 (build 21.0.13.0.3+1-openj9-0.63.0, JRE 21 Windows 11 '
           'amd64-64-Bit Compressed References 20261024_885 (JIT enabled, AOT enabled)\n'
           'OpenJ9   - dcc7ee16aa\n'
           'OMR      - c2a3a9ae0\n'
           'JCL      - 62d1b70e191 based on jdk-21.0.13.0.3+1)'}
ROLE_HASHES = {'zulu': {'java/lang/Throwable': '0f7e3970a4957698dd14c774ee8200407669a91346747c98cfa6c41150e76e51',
          'java/lang/Exception': '402055b2940a9bd899905efbce93d086eeead51d6c16276735add88dad651348',
          'java/lang/RuntimeException': '04170ba5f8b615d2550bfb22ddd4cf2a3090660333ffcc557c6a2e5015d939a2',
          'java/lang/ArithmeticException': '73612a7fa40ac9c1f13b899a8b708c26445a6496fd2d793c40391a09b1899710',
          'java/lang/StackTraceElement': '815d372dbb58a7fe77c7cdf49060bceb471cd92ecfbd77da233d8775684509d6',
          'java/util/Collections': '5c75d4e022fa21c95ae63a1bde0f44e837b5f555c6a35f3e9389d7aff1ce579f',
          'java/util/Collections$EmptySet': 'b09df432c411a8e2e0d74a20f84f16fdf108f836bb33e922c2a56c4dea1804de',
          'java/util/Collections$EmptyList': '8523c61e293f447611bc8f053e64cb27422f9bc42f81234ccc95df394b06aa75',
          'java/util/Collections$EmptyMap': '1927161b7ae5643323a054148d41d3f8be0751046144f028cbac6c2f159282f3'},
 'openj9': {'java/lang/Throwable': 'ccf885b75316ac083d2b8ffeb1eb7de522faf6927f74079209b265c9a77b9848',
            'java/lang/Exception': '21d04204ca7f1e7bddab97d45d8296dee76e39cbc81cd21ffa80ee71e6fffe75',
            'java/lang/RuntimeException': 'fce97249caf52f42f0fb0b4c3387ed65cba5d70cdf28a97d6854a7a04f320e51',
            'java/lang/ArithmeticException': '7363bdab0716ff04ea9c09d7028fd830ebbce3821c22c4f4dfe3d67b11491488',
            'java/lang/StackTraceElement': '5043e5cfff9564e0b98f9f6891b447f5ba876e2c60bab70fa21f2fb0da154869',
            'java/util/Collections': 'd3c5995e9b279bebacbe3824cf90da5cd44cd897276f869448a4265d5bcf6e0a',
            'java/util/Collections$EmptySet': 'e1737d2eaafc0a37301e8fa656d1682d16656b3656709c9eeb8b26fe499e17c8',
            'java/util/Collections$EmptyList': '2369ba005df473da188094a2d011df527e070ad683060fb80828d54ea8fdbb5f',
            'java/util/Collections$EmptyMap': '8d4458230fd98ffb0e641d43b6611c4ccb23d935c0e8909837724cf4633fbafd'}}

def portable(rows):
    out = []
    for r in rows:
        if r['case'] == 'boundaries':
            if not r['values'] == '-2147483648/0/-9223372036854775808/0':
                raise EvidenceError('portable Throwable observation differs')
            out.append(r)
            continue
        if not (r['sameObject'] == 'true' and r['causeIdentity'] == 'true'):
            raise EvidenceError('portable Throwable observation differs')
        if not int(r['traceSize']) > 0:
            raise EvidenceError('portable Throwable observation differs')
        frames = r['trace'].split(',')
        top = frames[0].split('#')
        if not top[0].startswith('probe.ThrowableGuest'):
            raise EvidenceError('portable Throwable observation differs')
        case = r['case']
        expected = {'user-created': 'makeUser', 'user-rethrow': 'makeUser', 'user-refill': 'refill', 'user-rethrow-after-refill': 'refill', 'user-cause': 'refill', 'constructor-replacement': 'refill', 'escaped-discarded': 'constructorReplacement', 'constructor-vm-fault': '<init>', 'escaped-vm-constructor': 'constructorVMFailure', 'vm-rethrow': 'idiv', 'vm-refill': 'refill'}
        if case in expected:
            if not top[1] == expected[case]:
                raise EvidenceError(repr(r))
        if re.fullmatch('[il](?:div|rem)-[01]', case):
            if not (top[0] == 'probe.ThrowableGuest' and top[1] == case[:-2]):
                raise EvidenceError('portable Throwable observation differs')
            if not (r['class'] == 'java.lang.ArithmeticException' and r['exactArithmetic'] == 'true' and (r['fresh'] == 'true')):
                raise EvidenceError('portable Throwable observation differs')
            if not (r['cause'] == 'null' and r['after'] == '0' and (r['catchCount'] == '1') and (r['mismatch'] == '0')):
                raise EvidenceError('portable Throwable observation differs')
            if not int(r['before']) == {'idiv': 1, 'irem': 2, 'ldiv': 3, 'lrem': 4}[case[:-2]] * 1000 + 1:
                raise EvidenceError('portable Throwable observation differs')
        if case in ['user-rethrow', 'user-rethrow-after-refill', 'user-cause', 'constructor-replacement', 'vm-rethrow']:
            if not r['traceStable'] == 'true':
                raise EvidenceError(repr(r))
        if case in ['user-refill', 'vm-refill']:
            if not r['traceChanged'] == 'true':
                raise EvidenceError(repr(r))
        if case == 'user-created':
            if not (r['writes'] == '11' and r['cause'] == 'null' and (r['message16'] == '00670075006500730074003a0000dfff20ac')):
                raise EvidenceError('portable Throwable observation differs')
        if case == 'user-rethrow':
            if not r['writes'] == '23':
                raise EvidenceError('portable Throwable observation differs')
        if case == 'constructor-replacement':
            if not (r['writes'] == '9101' and r['cause'] == 'java.lang.RuntimeException'):
                raise EvidenceError('portable Throwable observation differs')
        if case == 'constructor-vm-fault':
            if not (r['writes'] == '9201' and top[0] == 'probe.ThrowableGuest$VMFailingCtor'):
                raise EvidenceError('portable Throwable observation differs')
        keep = {k: v for k, v in r.items() if k not in ['message16', 'trace', 'traceSize']}
        keep['traceTopClass'] = top[0]
        keep['traceTopMethod'] = top[1]
        if not r['class'] == 'java.lang.ArithmeticException':
            keep['message16'] = r['message16']
        out.append(keep)
    if not len(out) == 20:
        raise EvidenceError('portable Throwable observation differs')
    return out

def filter_portable(rows):
    if len(rows) != 7:
        raise EvidenceError('trace-filter row count')
    values = {r['case']: r for r in rows}
    a = values['unrelated-throwable-constructor']
    b = values['unrelated-ordinary-constructor']
    if not a['inner'].startswith('TraceFilterObserver$OuterFailure#<init>,'):
        raise EvidenceError('unrelated Throwable constructor removed')
    if a['outer'] != 'TraceFilterObserver#main':
        raise EvidenceError('own constructor not filtered')
    if not b['inner'].startswith('TraceFilterObserver$PlainOuter#<init>,'):
        raise EvidenceError('ordinary constructor removed')
    c = values['delegate-override']
    if (c['calls'], c['heldInside'], c['heldAfter']) != ('1', 'true', 'false'):
        raise EvidenceError('override/monitor behavior differs')
    if values['bypass-override'] != {'case': 'bypass-override', 'trace': '', 'calls': '1'}:
        raise EvidenceError('bypass override captured a trace')
    if values['non-writable'] != {'case': 'non-writable', 'trace': '', 'same': 'true', 'traceAfter': ''}:
        raise EvidenceError('nonwritable trace changed')
    if values['covariant-override']['calls'] != '1':
        raise EvidenceError('covariant override did not execute once')
    if (values['overload-flow']['overrides'], values['overload-flow']['helpers']) != ('1', '1'):
        raise EvidenceError('overload flow did not execute')
    return {'lower_unrelated_Throwable_constructor_retained': True, 'lower_ordinary_constructor_retained': True, 'override_calls': 1, 'override_monitor_held_inside': True, 'override_monitor_released': True, 'bypass_trace_empty': True, 'nonwritable_trace_empty': True, 'covariant_override_calls': 1, 'overload_override_calls': 1, 'overload_helper_calls': 1}

def compare_run_diagnostics(runs, filter_runs, strict):
    if strict and (any(r != runs[0] for r in runs) or any(r != filter_runs[0] for r in filter_runs)):
        raise EvidenceError('original/rebuilt/interpreted diagnostics differ within strict vendor')

def serialize(data):
    return json.dumps(data, indent=2, sort_keys=True) + '\n'

def reference_tool(name):
    home = os.environ.get('JAVA_HOME')
    path = shutil.which(name, path=str(Path(home) / 'bin') if home else None)
    if path is None:
        raise EvidenceError('missing Java21 reference tool ' + name)
    return Path(path)

def replay(args, work):
    pack = args.fixtures
    # Capture and authenticate every execution input before any subprocess.
    history = load_history(pack)
    blobs = capture_inputs(pack)
    captured_hashes = blob_hashes(blobs)
    tool_hash = tool_digest(TOOL)
    reviewed = bool(args.reviewed_source_update)
    if args.check and reviewed:
        raise EvidenceError('reviewed replacement is not check mode')
    if args.portable_primary and (not args.check):
        raise EvidenceError('portable mode checks an existing recording only')
    if args.check and history is None:
        raise EvidenceError('missing prior recording')
    helper_hash = tool_digest(Path(read_java_source.__code__.co_filename))
    validate_capture(blobs, history, tool_hash, reviewed, helper_hash)
    if not args.check and args.compare_openj9 is None:
        raise EvidenceError('recording requires both pinned VMs')
    if not args.check and history and (set(history['manifest']['runtime']) != {'zulu', 'openj9'}):
        raise EvidenceError('incomplete recording')
    commands = []
    observations = {}
    filters = {}
    runtime = {}
    required = None

    def run(argv, label):
        argv = list(map(str, argv))
        stem = Path(argv[0]).stem.lower()
        if stem in ['javac', 'javap']:
            argv[1:1] = ['-J-Dfile.encoding=UTF-8', '-J-Dsun.stdout.encoding=UTF-8', '-J-Dsun.stderr.encoding=UTF-8']
        elif stem == 'java':
            argv[1:1] = ['-Dfile.encoding=UTF-8']
        result = subprocess.run(argv, capture_output=True, timeout=90)
        (work / (label + '.stdout')).write_bytes(result.stdout)
        (work / (label + '.stderr')).write_bytes(result.stderr)
        commands.append({'command': argv, 'exit_code': result.returncode, 'stdout_sha256': digest(result.stdout), 'stderr_sha256': digest(result.stderr)})
        if result.returncode:
            raise EvidenceError(label + ' exited ' + str(result.returncode))
        return (result.stdout + result.stderr).decode('utf-8').replace('\r\n', '\n')
    with tempfile.TemporaryDirectory(prefix='captured-', dir=work) as temp:
        scratch = Path(temp)
        stage_inputs(blobs, scratch)
        original = scratch / 'original'
        rebuilt = scratch / 'rebuilt'
        rebuilt.mkdir()
        controls = scratch / 'observer-classes'
        controls.mkdir()
        compiler = run([args.javac, '-version'], 'compiler-version').strip()
        if not re.match('^javac 21(?:[. ]|$)', compiler):
            raise EvidenceError('Java21 compiler required')
        compiler_hash = digest(args.javac.read_bytes())
        compiler_pin = requires_exact_rebuild(compiler, compiler_hash, history['manifest']) if history else compiler == VERSIONS['javac']
        if not args.check and (not compiler_pin):
            raise EvidenceError('recording requires the pinned compiler')
        run([args.javac, *FLAGS, '-d', rebuilt, *[scratch / 'src' / n for n in sorted(GUEST)]], 'compile-guest')
        rebuilt_hashes = inventory(rebuilt)
        if set(rebuilt_hashes) != CLASS_NAMES:
            raise EvidenceError('unexpected compiler guest class inventory')
        if not blobs['classes']:
            if history:
                raise EvidenceError('recording has no original guest bytes')
            for name in sorted(CLASS_NAMES):
                path = original / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes((rebuilt / name).read_bytes())
        else:
            check_rebuild(captured_hashes['classes'], rebuilt_hashes, compiler_pin and (not reviewed))
        run([args.javac, *FLAGS, '-cp', rebuilt, '-d', controls, *[scratch / 'controls' / n for n in sorted(CONTROLS)]], 'compile-observers')
        vendors = [('zulu', args.java, [])] + ([('openj9', args.compare_openj9, ['-Xshareclasses:none'])] if args.compare_openj9 else [])
        for vendor, vm, opts in vendors:
            version = run([vm, '-version'], vendor + '-version').strip()
            if not re.match('^(?:openjdk|java) version "21(?:[."+\\-]|$)', version):
                raise EvidenceError('Java21 runtime required')
            pinned_runtime = version == VERSIONS[vendor]
            strict = not args.portable_primary and pinned_runtime and compiler_pin
            if not args.check and (not strict):
                raise EvidenceError('recording requires the pinned runtime')
            cp = os.pathsep.join(map(str, [controls, original]))
            identity = run([vm, *opts, '-cp', cp, 'PlatformIdentity'], vendor + '-platform').splitlines()
            values = dict((x.split('=', 1) for x in identity))
            resources = {k: v for k, v in values.items() if not k.startswith('property:')}
            if strict and resources != ROLE_HASHES[vendor]:
                raise EvidenceError(vendor + ' platform class bytes differ')
            if not args.check and history and (resources != history['manifest']['runtime'][vendor]['resources']):
                raise EvidenceError('historical platform role pins differ')
            home = vm.resolve().parent.parent
            runtime[vendor] = {'version': version, 'executable_sha256': digest(vm.read_bytes()), 'resources': resources, 'properties': {k[9:]: v for k, v in values.items() if k.startswith('property:')}, 'files': {name: digest((home / name).read_bytes()) for name in ['bin/java.exe', 'release', 'lib/modules'] if (home / name).is_file()}, 'strict_diagnostics': strict}
            runs = []
            filter_runs = []
            for label, classes in [('original', original), ('rebuilt', rebuilt)]:
                cp = os.pathsep.join(map(str, [controls, classes]))
                for mode, extra in [('normal', []), ('interpreted', ['-Xint'])]:
                    key = vendor + '-' + label + '-' + mode
                    rows = parse(run([vm, *opts, *extra, '-cp', cp, 'probe.ObserveThrowable'], key))
                    portable_rows = portable(rows)
                    filter_rows = parse(run([vm, *opts, *extra, '-cp', cp, 'TraceFilterObserver'], key + '-filters'))
                    portable_filters = filter_portable(filter_rows)
                    value = {'throwable': portable_rows, 'filters': portable_filters}
                    if required is not None and value != required:
                        raise EvidenceError('observed semantic/diagnostic envelope differs')
                    required = value
                    runs.append(rows)
                    filter_runs.append(filter_rows)
            compare_run_diagnostics(runs, filter_runs, strict)
            observations[vendor] = runs[0]
            filters[vendor] = filter_runs[0]
            if history and (not args.check or strict):
                if history['outputs'][f'observations/{vendor}.json'] != runs[0]:
                    raise EvidenceError('historical Throwable observations differ')
                if history['outputs'][f'filters/{vendor}.json'] != filter_runs[0]:
                    raise EvidenceError('historical filter observations differ')
        if history and history['outputs']['portable.json'] != required:
            raise EvidenceError('historical observed envelope differs')
        try:
            unchanged = inputs(pack) == captured_hashes
        except (ValueError, OSError):
            unchanged = False
        if not unchanged:
            raise EvidenceError('live archive inputs changed during replay; captured execution was isolated')
        if history and load_history(pack) != history:
            raise EvidenceError('historical evidence changed during replay')
        wrote = False
        if not args.check and (history is None or reviewed):
            (pack / 'portable.json').write_bytes(serialize(required).encode('utf-8'))
            for folder, data in [('observations', observations), ('filters', filters)]:
                (pack / folder).mkdir(exist_ok=True)
                for vendor, rows in data.items():
                    (pack / folder / f'{vendor}.json').write_bytes(serialize(rows).encode('utf-8'))
            if history is None or reviewed:
                for name in sorted(CLASS_NAMES):
                    path = pack / 'classes' / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes((rebuilt / name).read_bytes())
            manifest = {'schema': 2, 'scope': 'Original Java21 reference fixtures, not CrabSpot execution', 'compiler': compiler, 'compiler_sha256': compiler_hash, 'compiler_flags': FLAGS, 'inputs': inputs(pack), 'outputs': {n: digest((pack / n).read_bytes()) for n in sorted(OUTPUTS)}, 'runtime': runtime, 'tool_sha256': tool_hash, 'fixture_source_sha256': tool_digest(Path(read_java_source.__code__.co_filename)), 'diagnostics': 'Nonempty trace/top/filter expectations are a declared observed reference envelope, not universal Java conformance.'}
            if history:
                manifest.update(supersedes_manifest_sha256=history['manifest_sha256'], reviewed_source_update=args.reviewed_source_update)
            (pack / 'manifest.json').write_bytes(serialize(manifest).encode('utf-8'))
            wrote = True
    evidence = {'mode': 'check' if args.check else 'reviewed-source-update' if reviewed else 'record', 'commands': commands, 'runtime': runtime, 'captured_inputs': captured_hashes, 'compiler_exact_rebuild': compiler_pin, 'original_rebuilt_verified': True, 'throwable_rows': 20, 'filter_rows': 7, 'manifest_sha256': digest((pack / 'manifest.json').read_bytes()), 'tool_sha256': tool_hash, 'recording_written': wrote}
    (work / 'last-run.json').write_bytes(serialize(evidence).encode('utf-8'))
    print(serialize({k: evidence[k] for k in ['mode', 'compiler_exact_rebuild', 'throwable_rows', 'filter_rows', 'manifest_sha256', 'recording_written']}))
    return evidence

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--java', type=Path)
    parser.add_argument('--javac', type=Path)
    parser.add_argument('--compare-openj9', type=Path)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--portable-primary', action='store_true')
    parser.add_argument('--fixtures', type=Path, default=PACK)
    parser.add_argument('--work', type=Path)
    parser.add_argument('--reviewed-source-update', metavar='REASON', help='Explicit reviewed source/tool update; all historical rows and role pins must remain unchanged')
    args = parser.parse_args(argv)
    args.java = args.java or reference_tool('java')
    args.javac = args.javac or reference_tool('javac')
    if args.work is None:
        root = ROOT / 'target/fixture-tool'
        root.mkdir(parents=True, exist_ok=True)
        context = tempfile.TemporaryDirectory(prefix='throwable-', dir=root)
    else:
        args.work.mkdir(parents=True, exist_ok=True)
        context = nullcontext(str(args.work))
    with context as work:
        return replay(args, Path(work))
if __name__ == '__main__':
    main()
