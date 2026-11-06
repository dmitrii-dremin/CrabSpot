"""Replay genuine Throwable applications through the public API and physical CLI.

Java VMs supply independent fixture observations only. Application execution on
CrabSpot uses the normal directory entry and the Rust public API example.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import subprocess
import tempfile

import throwable_entry_reference as reference
from type_state_encodings import ClassEncoding
from verify_application_entry import extract_classes
from verify_java_base import pack_archive
from verify_string_entry import physical_entry

ROOT = Path(__file__).resolve().parents[1]
COUNTERS = ('proved', 'witnesses', 'admitted', 'argv', 'arguments', 'evaluations',
            'discovery', 'snapshots', 'frames', 'backtraces', 'trace_work', 'started', 'produced')
MAIN = [65535, 8, 4, 1]
MAIN_STATUS = 'returned'
WRONG = ('MainEvaluation uncaught probe/ThrowableEntryGuest$UserFailure at '
         'app/ThrowableMain.monitorRefill(Ljava/lang/Throwable;)Z@49')
UNCAUGHT = ('MainEvaluation uncaught java/lang/ArithmeticException at '
            'probe/ThrowableEntryGuest.idiv(II)I@8')
REFILLED = ('MainEvaluation uncaught probe/ThrowableEntryGuest$UserFailure at '
            'probe/ThrowableEntryGuest.refill(Ljava/lang/RuntimeException;)Ljava/lang/Throwable;@6')
TRACE = ('MainEvaluation failed initStackTraceElements([Ljava/lang/StackTraceElement;'
         'Ljava/lang/Object;I)V@entry via=of(Ljava/lang/Object;I)[Ljava/lang/StackTraceElement;'
         '@31 fault=none owner=true call-owner-same=true call-method-distinct=true UnsupportedMethod native methods require runtime services')


def digest(data):
    return hashlib.sha256(data).hexdigest()


def process(command, code=0, diagnostic=None):
    result = subprocess.run([str(x) for x in command], capture_output=True, timeout=240)
    out, err = result.stdout.decode('utf8'), result.stderr.decode('utf8')
    expected = () if diagnostic is None else (diagnostic,) if isinstance(diagnostic,str) else diagnostic
    if result.returncode != code or (diagnostic is None and err) or any(part not in err for part in expected):
        raise ValueError(f'command failed {command!r}: exit={result.returncode}, stderr={err!r}')
    return out


def parse_output(output):
    rows = output.splitlines()
    if len(rows) not in (2, 3):
        raise ValueError('invalid public result row count')
    pairs = [token.partition('=') for token in rows[-1].split()]
    if tuple(k for k, _, _ in pairs) != COUNTERS or any(
            mark != '=' or not value.isascii() or not value.isdecimal() or int(value) > 2**64-1
            for _, mark, value in pairs):
        raise ValueError('invalid public counters')
    usage = {key: int(value) for key, _, value in pairs}
    fields = None
    if len(rows) == 3:
        if not rows[1].startswith('fields='):
            raise ValueError('missing public field row')
        fields = json.loads(rows[1][len('fields='):])
        if not isinstance(fields, list) or any(type(v) is not int for v in fields):
            raise ValueError('invalid public fields')
        if not usage['proved'] or usage['proved'] != usage['witnesses'] or usage['proved'] != usage['admitted']:
            raise ValueError('inconsistent proof/witness/layout authority')
        if usage['argv'] != 1 or usage['evaluations'] != 1:
            raise ValueError('execution did not use one actual argument array and evaluation')
    else:
        effects=('proved','witnesses','admitted','argv','arguments','evaluations','snapshots','frames','backtraces','trace_work','started','produced')
        if not rows[0].startswith('image ') or any(usage[k] for k in effects):
            raise ValueError('preflight image failure has proof or executable effects')
    if usage['produced'] > usage['started'] or usage['backtraces'] > usage['snapshots']:
        raise ValueError('inconsistent production/trace counters')
    return {'status': rows[0], 'fields': fields, 'usage': usage}


def expect(row, status, fields, **counters):
    if row['status'] != status or row['fields'] != fields:
        raise ValueError(f'expected {status!r}/{fields}, observed {row}')
    if any(row['usage'][key] != value for key, value in counters.items()):
        raise ValueError(f'expected counters {counters}, observed {row}')
    return row['usage']


def snapshot_directory(directory, max_files=20000, max_bytes=134217728):
    """Capture once, then execute only newly staged checked buffers."""
    directory = Path(directory)
    blobs, total = {}, 0
    def regular(path, want_dir):
        info = path.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info, 'st_file_attributes', 0) & 0x400:
            raise ValueError('linked/reparse platform input')
        if not (stat.S_ISDIR(info.st_mode) if want_dir else stat.S_ISREG(info.st_mode)):
            raise ValueError('special platform input')
        return info
    regular(directory, True)
    for parent, dirs, files in os.walk(directory, followlinks=False):
        for name in dirs:
            regular(Path(parent)/name, True)
        for name in sorted(files):
            path = Path(parent)/name
            info = regular(path, False)
            if len(blobs) >= max_files or info.st_size > max_bytes-total:
                raise ValueError('platform snapshot limit')
            with path.open('rb') as source:
                data = source.read(max_bytes-total+1)
            if len(data) != info.st_size or len(data) > max_bytes-total:
                raise ValueError('platform snapshot size changed or limit')
            key = path.relative_to(directory).as_posix()
            blobs[key] = data
            total += len(data)
    if 'java/lang/Object.class' not in blobs or 'java/lang/Throwable.class' not in blobs:
        raise ValueError('incomplete supplied platform')
    return blobs


def stage(blobs, directory):
    directory.mkdir(parents=True, exist_ok=True)
    for name, raw in sorted(blobs.items()):
        path = directory/name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(raw)
    return directory


def verify_directories(expected):
    for directory, hashes in expected:
        if directory.is_symlink() or getattr(directory.lstat(),'st_file_attributes',0)&0x400:
            raise ValueError('captured directory became a link/reparse')
        if reference.hashes(directory) != hashes:
            raise ValueError('captured input inventory/bytes changed: '+str(directory))


def verify_files(expected):
    for path, expected_hash in expected.items():
        if path.is_symlink() or digest(path.read_bytes()) != expected_hash:
            raise ValueError('captured tool or input changed: '+str(path))


def helper_sources():
    paths={Path(__file__).resolve(), ROOT/'crates/launcher/examples/replay_throwable_entry.rs'}
    for module in list(sys.modules.values()):
        filename=getattr(module,'__file__',None)
        if filename:
            path=Path(filename).resolve()
            if path.suffix=='.py' and ROOT/'tools' in path.parents:
                paths.add(path)
    return {path:digest(path.read_bytes()) for path in sorted(paths)}


def native_flags_offset(raw):
    """Locate a genuine exact ABI slot; Code and attributes are skipped unchanged."""
    def u2(at):
        if at < 0 or at+2 > len(raw):
            raise ValueError('truncated class metadata')
        return int.from_bytes(raw[at:at+2], 'big')
    if raw[:4] != bytes.fromhex('cafebabe'):
        raise ValueError('invalid class magic')
    pool, at, index, count = {}, 10, 1, u2(8)
    while index < count:
        if at >= len(raw):
            raise ValueError('truncated constant pool')
        tag = raw[at]
        at += 1
        if tag == 1:
            size = u2(at)
            at += 2
            if at+size > len(raw):
                raise ValueError('truncated UTF constant')
            pool[index] = raw[at:at+size]
        elif tag in (3,4,9,10,11,12,17,18):
            size = 4
        elif tag in (5,6):
            size = 8
            index += 1
        elif tag in (7,8,16,19,20):
            size = 2
        elif tag == 15:
            size = 3
        else:
            raise ValueError('unknown constant pool tag')
        at += size
        if at > len(raw):
            raise ValueError('truncated constant pool payload')
        index += 1
    at += 6
    at += 2+2*u2(at)
    matches = []
    for section in range(2):
        members = u2(at)
        at += 2
        for _ in range(members):
            flags_at = at
            name, desc, attributes = u2(at+2), u2(at+4), u2(at+6)
            at += 8
            if section == 1 and pool.get(name) == b'fillInStackTrace' and pool.get(desc) == b'(I)Ljava/lang/Throwable;':
                matches.append(flags_at)
            for _ in range(attributes):
                u2(at)
                if at+6 > len(raw):
                    raise ValueError('truncated attribute')
                size = int.from_bytes(raw[at+2:at+6], 'big')
                at += 6+size
                if at > len(raw):
                    raise ValueError('truncated attribute body')
    if len(matches) != 1 or u2(matches[0]) != 0x102:
        raise ValueError('exact private-int-native ABI slot absent')
    return matches[0]


def independent_references(java, other, controls, originals, rebuilt):
    rows = []
    for vendor, vm, switches in [('primary', java, [])] + ([('openj9', other, ['-Xshareclasses:none'])] if other else []):
        version = reference.tool_version(vm)
        if vendor == 'openj9' and 'OpenJ9' not in version:
            raise ValueError('independent VM is not OpenJ9')
        for label, classes in [('archived', originals), ('rebuilt', rebuilt)]:
            for mode in ([], ['-Xint']):
                for case, expected in reference.EXPECTED.items():
                    stdout, stderr = reference.process([vm, *switches, *mode, '-Xverify:all', '-cp',
                            str(controls)+os.pathsep+str(classes), 'ObserveThrowableMain', case])
                    if stdout != expected:
                        raise ValueError(f'independent {vendor}/{label}/{mode}/{case}: {stdout!r} != {expected!r}')
                    rows.append({'vendor': vendor, 'version': version, 'input': label,
                                 'mode': mode, 'case': case, 'stdout': stdout, 'stderr': stderr})
    return rows


def suite(public, cli, platform, classes, work, label):
    rows = []
    def call(entry, *options):
        row = parse_output(process([public, platform, classes, 'app.'+entry, *options]))
        rows.append({'entry': entry, 'options': list(options), **row})
        return row
    normal = call('ThrowableMain')
    base = expect(normal, MAIN_STATUS, MAIN, started=13, produced=13, snapshots=23, frames=51, backtraces=23)
    if not base['discovery'] or not base['trace_work']:
        raise ValueError('matched profile has unfunded discovery/capture')
    expect(call('ThrowableMain', 'strict'), MAIN_STATUS, MAIN, started=13, produced=13)
    expect(call('ThrowableMain', 'wrong'), WRONG, [65535,8,4,0], started=13, produced=13)
    expect(call('ThrowableUncaught'), UNCAUGHT, [7,0], started=1, produced=1)
    expect(call('ThrowableRefilled'), REFILLED, [])
    expect(call('ThrowableTraceAccess'), TRACE, [7,0], started=0, produced=0)
    for key, cap, resource, counter in [('vm',13,'VmExceptionLimit','started'),
            ('snapshots',23,'Snapshots','snapshots'), ('frames',51,'Frames','frames'),
            ('trace-work',base['trace_work'],'TraceWork','trace_work')]:
        expect(call('ThrowableMain', f'{key}={cap}'), MAIN_STATUS, MAIN, started=13, produced=13)
        for limit in (cap-1,0):
            row = call('ThrowableMain', f'{key}={limit}')
            if not row['status'].startswith('MainEvaluation failed ') or not re.search(
                    rf' {resource} {limit}/(\d+)$', row['status']):
                raise ValueError('wrong resource failure: '+str(row))
            required = int(row['status'].rsplit('/',1)[1])
            if required <= limit or row['usage'][counter] > limit:
                raise ValueError('resource failure crossed limit or hid required count')
            if key == 'vm':
                expected = [255,8,3,0] if limit else [63,0,0,0]
                if row['fields'] != expected or row['usage']['started'] != limit or row['usage']['produced'] != limit:
                    raise ValueError('VM quota lost actual guest prefix')
                fault = 'ArithmeticException@66:Preflight:same-method=true' if limit else 'ArithmeticException@18:Preflight:same-method=true'
                method = 'local(II)Ljava/lang/ArithmeticException;@66' if limit else '<init>(I)V@18'
                if ('fault='+fault) not in row['status'] or not row['status'].startswith('MainEvaluation failed '+method+' ') or 'owner=true' not in row['status']:
                    raise ValueError('VM limit lost original fault provenance')
            else:
                if row['fields'] != ([32767,8,4,0] if limit else [0,0,0,0]):
                    raise ValueError('trace quota lost actual guest prefix: '+str(row))
                prefix = ((23,51,22) if key == 'trace-work' else (22,49,22)) if limit else (0,0,0)
                if tuple(row['usage'][k] for k in ('snapshots','frames','backtraces')) != prefix:
                    raise ValueError('trace quota lost its independently counted accepted prefix')
    # The original guest's deepest retained call is its constructor-fault chain:
    # VMFailingCtor -> constructorVMFailure -> main, independent of library names.
    expect(call('ThrowableMain','depth=3'), MAIN_STATUS, MAIN)
    for limit in (2,0):
        row = call('ThrowableMain',f'depth={limit}')
        if not re.search(rf' FramesPerSnapshot {limit}/[1-9]\d*$',row['status']):
            raise ValueError('per-snapshot depth did not reject before growth')
        if row['fields'] != ([63,0,0,0] if limit else [0,0,0,0]):
            raise ValueError('depth quota lost constructor-prefix effects')
    discovery = base['discovery']
    expect(call('ThrowableMain',f'discovery={discovery}'),MAIN_STATUS,MAIN)
    for limit in (discovery-1,0):
        row = call('ThrowableMain',f'discovery={limit}')
        if not re.fullmatch(rf'image Verification DiscoveryWork {limit}/\d+',row['status']):
            raise ValueError('discovery quota was not a preflight error')
        if row['fields'] is not None or row['usage']['discovery'] > limit or row['usage']['started']:
            raise ValueError('discovery quota initialized or materialized guest')
    # The observer class is not copied into this app inventory. These extra Java
    # guards use genuine original fields and divide by zero only on disagreement.
    for expected, success in [(MAIN,True),([65534,8,4,1],False)]:
        destination = work/f'physical-{label}-{int(success)}'
        physical_entry(destination,classes,'app.ThrowableMain','app/ThrowableMain',list(zip(
            ('flags','arithmeticCases','localCases','completed'),expected)))
        physical_hashes=reference.hashes(destination)
        output = process([cli,'run','--platform',platform,'--classes',destination,'physical.Entry','--'],
                         0 if success else 1,None if success else 'uncaught java/lang/ArithmeticException at physical/Entry.main([Ljava/lang/String;)V@20')
        verify_directories([(destination,physical_hashes)])
        if output:
            raise ValueError('physical guard printed unexpected output')
    for entry, diagnostic in [('ThrowableMain',None),('ThrowableUncaught','idiv(II)I@8'),
            ('ThrowableRefilled','refill(Ljava/lang/RuntimeException;)Ljava/lang/Throwable;@6'),
            ('ThrowableTraceAccess',('initStackTraceElements([Ljava/lang/StackTraceElement;Ljava/lang/Object;I)V', 'at bytecode offset 31: unsupported method body: native methods require runtime services'))]:
        output = process([cli,'run','--platform',platform,'--classes',classes,'app.'+entry,'--'],
                         0 if diagnostic is None else 1,diagnostic)
        if output:
            raise ValueError('physical CLI emitted unexpected stdout')
    if process([cli,'run','--platform',platform,'--classes',classes,'app.ThrowableMain','--','wrong'],
               1,WRONG.removeprefix('MainEvaluation ')):
        raise ValueError('physical wrong-argument guard emitted stdout')
    return rows


def profile_controls(public, platform, classes, blobs):
    rows = []
    def call(label,*options):
        r=parse_output(process([public,platform,classes,'app.ThrowableMain',*options]))
        rows.append({'case':label,'options':list(options),**r})
        return r
    disabled=call('disabled','disabled')
    if disabled['status']!='image Dependencies UnsupportedPlatformReference' or disabled['usage']['discovery']:
        raise ValueError('disabled profile gained imports or discovery')
    path=platform/'java/lang/Throwable.class'
    original=blobs['java/lang/Throwable.class']
    try:
        position=native_flags_offset(original)
        damaged=bytearray(original);damaged[position:position+2]=(0x101).to_bytes(2,'big');path.write_bytes(damaged)
        for options in ((),('strict',)):
            row=call("native-flags-mismatch",*options)
            if not row['status'].startswith('image Verification InvalidBinding ') or row['fields'] is not None:
                raise ValueError('claimed native mismatch silently fell back')
        # Remove the native's required name without inventing an implementation.
        absent=original.replace(b'fillInStackTrace',b'nopeInStackTrace')
        if absent==original:
            raise ValueError('missing native name in original ABI')
        path.write_bytes(absent)
        row=call('absent-native','strict')
        if row['status']!='image Verification UnsupportedShape':
            raise ValueError('absent native profile not reported strictly')
        row=call("absent-native")
        if row['status']!='image Dependencies UnsupportedPlatformReference':
            raise ValueError('absent profile granted new imports or fake native execution')
    finally:
        path.write_bytes(original)
    return rows


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--jdk',type=Path,default=os.environ.get('JAVA_HOME'))
    parser.add_argument('--platform',type=Path)
    parser.add_argument('--compare-openj9',type=Path)
    parser.add_argument('--openj9-platform',type=Path)
    parser.add_argument('--bin-dir',type=Path,help='use and record existing binaries instead of building release')
    parser.add_argument('--report',type=Path)
    args=parser.parse_args()
    if not args.jdk:
        parser.error('provide a Java21 JDK through --jdk or JAVA_HOME')
    suffix='.exe' if os.name=='nt' else ''
    java,javac=(args.jdk/'bin'/(name+suffix) for name in ('java','javac'))
    source_pins=helper_sources()
    executable_pins={path:digest(path.read_bytes()) for path in (java,javac)}
    if args.compare_openj9:executable_pins[args.compare_openj9]=digest(args.compare_openj9.read_bytes())
    primary=reference.tool_version(java)
    if 'OpenJ9' in primary:
        parser.error('primary supplied platform/reference must use HotSpot Java21')
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))
    if not target.is_absolute():target=ROOT/target
    if args.bin_dir:
        binaries=args.bin_dir.resolve()
    else:
        subprocess.run(['cargo','build','--release','--locked','-j1','-p','crabspot-launcher',
                        '--bin','crabspot','--example','replay_throwable_entry'],cwd=ROOT,check=True)
        binaries=target/'release'
    public=binaries/'examples'/('replay_throwable_entry'+suffix)
    cli=binaries/('crabspot'+suffix)
    binary_hashes={str(p):digest(p.read_bytes()) for p in (public,cli)}
    scratch=target/'throwable-entry-check';scratch.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='public-',dir=scratch) as tmp:
        work=Path(tmp)
        captured,original,rebuilt,controls,pinned=reference.prepare(reference.PACK,work/'fixtures',javac)
        watched=[(reference.PACK,{name:digest(raw) for name,raw in captured[0].items()})]
        watched.extend((path,reference.hashes(path)) for path in (original,rebuilt,controls,work/'fixtures/src',work/'fixtures/controls'))
        historical=ROOT/'tests/fixtures/throwable'
        archive_manifest=captured[1]
        projection=json.loads(captured[0]['projection.json'])
        harness=json.loads(captured[0]['harness.json'])
        input_pins={historical/'manifest.json':harness['historical_manifest_sha256'],
                    historical/'src/probe/ThrowableGuest.java':projection['historical_sha256']}
        watched.append((historical/'classes',archive_manifest['historical_inputs']))
        refs=independent_references(java,args.compare_openj9,controls,original,rebuilt)
        supplied=args.platform
        if supplied is None:
            archive,supplied=work/'packed',work/'extracted'
            _,count,_=pack_archive(args.jdk/'jmods/java.base.jmod',archive)
            if extract_classes(archive,supplied)!=count+1:raise ValueError('incomplete platform extraction')
        blobs=snapshot_directory(supplied)
        platform=stage(blobs,work/'platform')
        expected_platform={name:digest(raw) for name,raw in blobs.items()}
        watched.extend(((supplied,expected_platform),(platform,expected_platform)))
        report={'compiler_exact_rebuild':pinned,'compiler':reference.tool_version(javac),
                'harness_sha256':source_pins[Path(__file__).resolve()],
                'source_sha256':{str(path):value for path,value in source_pins.items()},
                'reference_executable_sha256':{str(path):value for path,value in executable_pins.items()},
                'fixture_manifest_sha256':digest(captured[0]['manifest.json']),
                'reference_versions':{'primary':primary},
                'binary_sha256':binary_hashes,'platform_files':len(blobs),
                'platform_bytes':sum(map(len,blobs.values())),
                'platform_sha256':{name:digest(data) for name,data in sorted(blobs.items())},
                'reference_runs':refs,'product_runs':{}}
        for label,classes in [('archived',original),('rebuilt',rebuilt)]:
            report['product_runs'][label]=suite(public,cli,platform,classes,work,label)
        report['profile_controls']=profile_controls(public,platform,original,blobs)
        if args.openj9_platform:
            secondary_blobs=snapshot_directory(args.openj9_platform)
            secondary=stage(secondary_blobs,work/'openj9-platform')
            secondary_hashes={name:digest(raw) for name,raw in secondary_blobs.items()}
            watched.extend(((args.openj9_platform,secondary_hashes),(secondary,secondary_hashes)))
            for options,status in [((), 'image Dependencies UnsupportedPlatformReference'),
                    (('strict',),'image Verification UnsupportedShape')]:
                row=parse_output(process([public,secondary,original,'app.ThrowableMain',*options]))
                expect(row,status,None,started=0,produced=0,snapshots=0)
        verify_directories(watched)
        verify_files(source_pins)
        verify_files(executable_pins)
        verify_files(input_pins)
        if any(digest(p.read_bytes())!=binary_hashes[str(p)] for p in (public,cli)):
            raise ValueError('execution binary changed during replay')
        report['same_buffer_recheck']=True
        if args.report:
            args.report.parent.mkdir(parents=True,exist_ok=True)
            args.report.write_text(json.dumps(report,indent=2)+'\n',encoding='utf8')
        print(json.dumps({'reference_runs':len(refs),'product_runs':sum(map(len,report['product_runs'].values())),
                          'profile_controls':len(report['profile_controls']),
                          'compiler_exact_rebuild':pinned,'binary_sha256':binary_hashes},indent=2))

if __name__=='__main__':main()
