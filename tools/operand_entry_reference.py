"""Read-only replay of the immutable public operand fixture on Java 21 references.

Guest execution here is differential evidence only. CrabSpot never invokes this
module to implement a VM operation. Expected messages are independent vendor and
product policies; their equality is not inferred from a caught exception.
"""
import argparse
from dataclasses import dataclass
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
PACK = ROOT / 'tests/fixtures/operand-entry'
FLAGS = ['--release', '21', '-g', '-encoding', 'UTF-8']
LOCALE = ['-Duser.language=ru', '-Duser.country=RU', '-Dfile.encoding=UTF-8']
JAVAP_FLAGS = ['-J-Dfile.encoding=UTF-8', '-J-Dsun.stdout.encoding=UTF-8',
               '-J-Dsun.stderr.encoding=UTF-8', '-v', '-p', '-c', '-s', '-l']
PROTOTYPE = 'b7d59173da48edde649235beca345bfc7fb48a262890c3565bfe98fe2d22b7ee'
CONTRACT = '607c6d5fcb95c0ede8797d6d17b0513e2a8000abeb38eb5660f7b47e1bbe2be6'
SOURCES = {'app/OperandMain.java', 'app/OperandUncaught.java', 'operand/OperandGuest.java',
           'operand/Tag.java', 'operand/Tagged.java', 'operand/\u00c9l\u00e9ment.java', 'operand/\u540d.java'}
CLASSES = {n[:-5] + '.class' for n in SOURCES} | {'app/OperandMain$Failure.class'}
INPUTS = ({'source/' + n for n in SOURCES} | {'classes/' + n for n in CLASSES} |
          {'control/ObserveOperandMain.java', 'controls/ObserveOperandMain.class',
           'message-oracles.json', 'source-projection.json', 'provenance/prototype-v4-manifest.json',
           'provenance/transport-checks.json', 'provenance/debug-transport/result.json'})
METADATA = {'manifest.json', 'harness.json', 'reference.json', 'README.md'}
TOOLS = {'operand_entry_reference.py', 'test_operand_entry_reference.py', 'fixture_source.py'}
MAIN = {'normal': 'returned/184/42/6/8/8/248/29/1',
        'wrong-bounds': 'oracle-failed/0/0/0/0/0/0/0/0',
        'wrong-store': 'oracle-failed/184/42/0/0/0/226/18/0',
        'wrong-cast': 'oracle-failed/184/42/6/0/0/232/18/0',
        'wrong-arithmetic': 'oracle-failed/184/42/6/8/0/240/29/0'}
UNCAUGHT = {n: 'java.lang.' + cls + '/' + head + '/7/0/' + before + '/0'
            for n, cls, head, before in [
                ('bounds', 'ArrayIndexOutOfBoundsException', 'operand.OperandGuest.loadI', '11'),
                ('negative', 'NegativeArraySizeException', 'operand.OperandGuest.multiI', '31'),
                ('store', 'ArrayStoreException', 'operand.OperandGuest.storeA', '11'),
                ('cast', 'ClassCastException', 'operand.OperandGuest.castInt', '11'),
                ('null', 'NullPointerException', 'operand.OperandGuest.loadI', '11'),
                ('refilled', 'ArrayIndexOutOfBoundsException', 'app.OperandUncaught.main', '11')]}
CODECS = {'N': 'null', 'T': '', 'T0000dfff20ac': '0000dfff20ac',
          'Tffffd800dc00': 'ffffd800dc00', **{s: None for s in
          ['', 'Xabcd', 'T0', 'Tgggg', 'T+001', 'T-001', 'T000g', 'Nextra']}}
FAULT_OPS = set('iaload laload faload daload aaload baload caload saload iastore lastore '
                'fastore dastore aastore bastore castore sastore newarray anewarray '
                'multianewarray checkcast'.split())


def digest(data):
    return hashlib.sha256(data).hexdigest()


def lf(data):
    if b'\r' in data:
        raise ValueError('canonical LF artifact required')
    return data


def inventory(directory):
    result = {}
    for path in directory.rglob('*'):
        st = path.lstat()
        if path.is_symlink() or getattr(st, 'st_file_attributes', 0) & 1024:
            raise ValueError('link/reparse entry in captured inventory')
        if path.is_file():
            result[path.relative_to(directory).as_posix()] = path
    return result


def hashes(directory):
    return {n: digest(p.read_bytes()) for n, p in inventory(directory).items()}


@dataclass
class FixedTree:
    directory: Path
    expected: dict

    def check(self):
        if hashes(self.directory) != self.expected:
            raise ValueError('fixed captured tree changed: ' + str(self.directory))


@dataclass
class FixedFiles:
    expected: dict

    def check(self):
        if any(not p.is_file() or digest(p.read_bytes()) != h for p, h in self.expected.items()):
            raise ValueError('fixed staged metadata/arguments changed')


def runtime_hashes(directory):
    result = {}
    for p in directory.rglob('*'):
        n=p.relative_to(directory).as_posix()
        if p.is_symlink():
            target=p.resolve()
            if target.is_dir():
                raise ValueError('unsupported runtime directory symlink: '+str(p))
            result[n]={'link':os.readlink(p),'target':str(target),
                       'sha256':digest(target.read_bytes()) if target.is_file() else None}
        elif p.is_file():
            result[n]={'sha256':digest(p.read_bytes())}
    return result


@dataclass
class RuntimeTree:
    directory: Path
    expected: dict

    def check(self):
        if runtime_hashes(self.directory)!=self.expected:
            raise ValueError('runtime/tool inventory changed: '+str(self.directory))


@dataclass
class Capture:
    directory: Path
    blobs: dict
    manifest: dict
    tools: dict
    paths: dict

    def check(self):
        if set(inventory(self.directory)) != set(self.blobs):
            raise ValueError('live fixture inventory changed')
        if any(digest(p.read_bytes()) != digest(self.blobs[n]) for n, p in self.paths.items()):
            raise ValueError('live captured fixture changed')
        if any(digest(lf(p.read_bytes())) != h for p, h in self.tools.items()):
            raise ValueError('captured helper/tool changed')



def scope_runtimes(runtime_scope):
    if runtime_scope == 'dual': return ('zulu','openj9')
    if runtime_scope == 'primary-only': return ('zulu',)
    raise ValueError('unknown explicit runtime scope')


def validate_rows(rows, *, runtime_scope="dual"):
    expected = {(v, s, tuple(m), c) for v in scope_runtimes(runtime_scope)
                for s in ('archived', 'rebuilt') for m in ([], ['-Xint'])
                for c in list(MAIN) + list(UNCAUGHT)}
    seen = set()
    for row in rows:
        key = (row['vm'], row['input'], tuple(row['mode']), row['case'])
        wanted = MAIN.get(row['case'], UNCAUGHT.get(row['case']))
        if key not in expected or key in seen or row['exit_code'] != 0 or row['stdout'] != wanted:
            raise ValueError('wrong/incomplete operand reference observation')
        seen.add(key)
    if seen != expected:
        raise ValueError('missing operand reference observations')


def validate_codecs(rows, *, runtime_scope="dual"):
    expected = {(v, t) for v in scope_runtimes(runtime_scope) for t in CODECS}
    seen = set()
    for row in rows:
        key = (row['vm'], row['token'])
        if key not in expected or key in seen:
            raise ValueError('unexpected codec observation')
        wanted = CODECS[row['token']]
        if ((wanted is None and (row['exit_code'] == 0 or row['stdout'] or 'java.lang.IllegalArgumentException: invalid expected UTF16' not in row.get('stderr','') or 'ObserveOperandMain.decode' not in row.get('stderr',''))) or
                (wanted is not None and (row['exit_code'] != 0 or row['stdout'] != wanted))):
            raise ValueError('UTF16 observer boundary mismatch')
        seen.add(key)
    if seen != expected:
        raise ValueError('missing codec observations')



def validate_report(report, *, expected_scope):
    runtimes=scope_runtimes(expected_scope)
    if (report.get('runtime_scope') != expected_scope or
            report.get('required_runtimes') != list(runtimes) or
            type(report.get('secondary_requested')) is not bool or
            report['secondary_requested'] != (expected_scope=='dual') or
            report.get('expected_observations') != 44*len(runtimes) or
            report.get('expected_codec_checks') != 12*len(runtimes) or
            report.get('observations') != 44*len(runtimes) or
            report.get('codec_checks') != 12*len(runtimes) or
            set(report.get('versions',{})) != set(runtimes) or
            set(report.get('runtime_binaries',{})) != set(runtimes)):
        raise ValueError('report differs from invocation runtime scope')
    for name in runtimes:
        identity=report['runtime_binaries'][name]
        text=report['versions'][name]
        if (not isinstance(text,str) or not re.match(r'(?:openjdk|java) version "21[.\-"+]',text) or
                ('OpenJ9' in text) != (name=='openj9') or
                identity.get('version') != text or not identity.get('path') or
                not re.fullmatch(r'[0-9a-f]{64}',identity.get('sha256',''))):
            raise ValueError('missing/wrong actual runtime identity')
    validate_rows(report['runs'],runtime_scope=expected_scope)
    validate_codecs(report['codecs'],runtime_scope=expected_scope)


def capture(directory=PACK, root=ROOT):
    paths = inventory(directory)
    if set(paths) != INPUTS | METADATA:
        raise ValueError('unexpected operand fixture inventory')
    blobs = {n: read_java_source(p) if n.endswith('.java') else p.read_bytes()
             for n, p in sorted(paths.items())}
    for n, b in blobs.items():
        if not n.endswith('.class'):
            lf(b)
    manifest, harness = (json.loads(blobs[n]) for n in ('manifest.json', 'harness.json'))
    if (manifest['schema'] != 1 or set(manifest['files']) != INPUTS or
            manifest['flags'] != FLAGS or manifest['contract_sha256'] != CONTRACT):
        raise ValueError('operand manifest shape differs')
    if any(digest(blobs[n]) != h for n, h in manifest['files'].items()):
        raise ValueError('captured input hash mismatch')
    if harness['files'] != {n: digest(blobs[n]) for n in ['manifest.json', 'reference.json', 'README.md']}:
        raise ValueError('metadata hash mismatch')
    tools = {root / 'tools' / n: digest(lf((root / 'tools' / n).read_bytes())) for n in TOOLS}
    if {p.name: h for p, h in tools.items()} != harness['tools']:
        raise ValueError('tool provenance mismatch')
    raw = blobs['provenance/prototype-v4-manifest.json']
    if digest(raw) != PROTOTYPE:
        raise ValueError('prototype provenance differs')
    prototype = {r['path']: r['sha256'].lower() for r in json.loads(raw)['files']}
    for n in INPUTS - {'provenance/prototype-v4-manifest.json'}:
        original = n.removeprefix('provenance/')
        if digest(blobs[n]) != prototype[original]:
            raise ValueError('prototype bytes changed: ' + n)
    projection = json.loads(blobs['source-projection.json'])
    for row in projection['unchanged_helper_sources']:
        if row['transform'] != 'none' or digest(blobs[row['path']]) != row['sha256']:
            raise ValueError('helper source projection differs')
    for row in projection['unchanged_helper_classes']:
        if row['transform'] != 'none' or digest(blobs['classes/' + row['path']]) != row['sha256']:
            raise ValueError('helper class projection differs')
    reference = json.loads(blobs['reference.json'])
    validate_rows(reference['runs'])
    validate_codecs(reference['codecs'])
    return Capture(directory, blobs, manifest, tools, paths)


def encode(value):
    return 'N' if value is None else 'T' + value.encode('utf-16-be', 'surrogatepass').hex()


def parse_javap(text):
    result, method, desc = {}, None, None
    for line in text.splitlines():
        header = re.match(r'^  ((?:public|protected|private) \S.*\([^)]*\)(?: throws [^;]+)?;)$', line)
        if header:
            method = header[1].split('(')[0].split()[-1]
            desc = None
        if line.startswith('    descriptor: '):
            desc = line.split(': ', 1)[1]
        match = re.match(r'^\s+(\d+):\s+(\w+)', line)
        if match and match[2] in FAULT_OPS | {'idiv', 'irem', 'ldiv', 'lrem'}:
            if method is None or desc is None:
                raise ValueError('unassociated javap instruction')
            result.setdefault(method + desc, []).append([int(match[1]), match[2]])
    return result


def required_sites():
    result = {}
    for tag, op in zip('ZBCSIJFDA', ['b', 'b', 'c', 's', 'i', 'l', 'f', 'd', 'a']):
        t = 'Ljava/lang/Object;' if tag == 'A' else tag
        result['load' + tag + '([' + t + 'I)' + t] = [[7, op + 'aload']]
        result['store' + tag + '([' + t + 'I' + t + ')V'] = [[8, op + 'astore']]
        result['new' + tag + '(I)[' + t] = [[6, 'anewarray' if tag == 'A' else 'newarray']]
    result['multiI(III)[[[I'] = [[8, 'multianewarray']]
    result['multiA(II)[[[Ljava/lang/String;'] = [[7, 'multianewarray']]
    for n in ['String', 'Tag', 'Ref', 'ObjectArray', 'ObjectRank', 'Int', 'IntRank',
              'RefRank', 'Clone', 'Serial', 'Unicode']:
        result['cast' + n + '(Ljava/lang/Object;)Ljava/lang/Object;'] = [[6, 'checkcast']]
    return result


def inspect_sites(outputs):
    helper = parse_javap(outputs['operand.OperandGuest'])
    if helper != required_sites() or {op for rows in helper.values() for _, op in rows} != FAULT_OPS:
        raise ValueError('helper fault opcode/descriptor/BCI mismatch')
    entry = parse_javap(outputs['app.OperandMain'])
    if entry.get('arithmetic(II)J') != [[35, 'idiv'], [41, 'irem'], [49, 'ldiv'], [56, 'lrem']]:
        raise ValueError('entry arithmetic sites differ')
    if [279, 'iaload'] not in entry.get('boundsCases()V', []):
        raise ValueError('entry direct iaload site differs')
    return {n: parse_javap(t) for n, t in outputs.items()}


def version(result):
    text = (result['stdout'] + '\n' + result['stderr']).strip()
    if not re.match(r'(?:javac |(?:openjdk|java) version ")21(?:[.\-"+]|$)', text):
        raise ValueError('Java 21 tool required: ' + text)
    return text


def check_rebuilt(captured, compiler, compiler_hash, guest, controls):
    if set(guest) != CLASSES or set(controls) != {'ObserveOperandMain.class'}:
        raise ValueError('unexpected compiler output inventory')
    pinned = (compiler == captured.manifest['compiler'] and compiler_hash == captured.manifest['compiler_sha256'])
    if pinned and (guest != {n: captured.manifest['files']['classes/' + n] for n in CLASSES} or
                   controls != {'ObserveOperandMain.class': captured.manifest['files']['controls/ObserveOperandMain.class']}):
        raise ValueError('pinned compiler byte reproduction failed')
    return pinned


def tool_hash(path):
    path=Path(path)
    return digest(lf(path.read_bytes()) if path.suffix=='.py' else path.read_bytes())


def tool_version(path):
    result=subprocess.run([str(path),'-version'],capture_output=True,timeout=30)
    if result.returncode: raise ValueError('tool version failed')
    return version({'stdout':result.stdout.decode('utf8').strip(),'stderr':result.stderr.decode('utf8').strip()})


def product_arguments(captured,case='normal'):
    messages=json.loads(captured.blobs['message-oracles.json'])['crabspot']['messages']
    result=[]
    for value in messages:
        result.extend(['null',''] if value is None else ['text',value])
    if case=='wrong-bounds': result[1]='wrong bounds message'
    elif case=='wrong-store': result[18:20]=['text','wrong store message']
    elif case=='wrong-cast': result[25]='wrong cast message'
    elif case=='wrong-arithmetic': result[33]='wrong arithmetic message'
    elif case!='normal': raise ValueError('unknown product oracle case')
    return result


class PreparedRun:
    def __init__(self,directory,root,work,tools,captured=None):
        self.captured=captured if captured is not None else capture(directory,root)
        if self.captured.directory.resolve()!=Path(directory).resolve(): raise ValueError('capture directory differs')
        self.captured.check()
        self.vm_trees={str(p.parent.parent):RuntimeTree(p.parent.parent,runtime_hashes(p.parent.parent)) for p in tools}
        self.guards=[]
        self.commands=[]
        self.work=Path(work)
        if self.work.exists() and any(self.work.iterdir()): raise ValueError('staging directory must be empty')
        self.work.mkdir(parents=True,exist_ok=True)
        for n,data in self.captured.blobs.items():
            dest=self.work/n;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
        for group in ['source','classes','control','controls','provenance']:
            expected={n.removeprefix(group+'/'):digest(b) for n,b in self.captured.blobs.items() if n.startswith(group+'/')}
            self.guards.append(FixedTree(self.work/group,expected))
        self.guards.append(FixedFiles({self.work/n:digest(b) for n,b in self.captured.blobs.items() if '/' not in n}))

    def verify(self):
        self.captured.check()
        for guard in self.guards: guard.check()

    def command(self,args,allow_failure=False):
        self.verify()
        result=subprocess.run([str(a) for a in args],capture_output=True,timeout=120)
        self.verify()
        row={'command':[str(a) for a in args],'exit_code':result.returncode,
             'stdout':result.stdout.decode('utf8').strip(),'stderr':result.stderr.decode('utf8').strip()}
        self.commands.append(row)
        if result.returncode and not allow_failure: raise ValueError('reference subprocess failed: '+repr(row))
        return row

    def compile(self,javac):
        self.compiler=version(self.command([javac,'-version']))
        for output,args in [('rebuilt',[self.work/'source'/n for n in sorted(SOURCES)]),
                            ('rebuilt-controls',['-cp',self.work/'classes',self.work/'control/ObserveOperandMain.java'])]:
            argfile=self.work/(output+'.args')
            tokens=[*FLAGS,'-d',self.work/output,*args]
            argfile.write_text('\n'.join('"'+str(x).replace('\\','/').replace('"','\\"')+'"' for x in tokens)+'\n',encoding='utf8',newline='\n')
            self.guards.append(FixedFiles({argfile:digest(argfile.read_bytes())}))
            self.command([javac,'@'+str(argfile)])
            self.guards.append(FixedTree(self.work/output,hashes(self.work/output)))
        self.pinned=check_rebuilt(self.captured,self.compiler,digest(javac.read_bytes()),hashes(self.work/'rebuilt'),hashes(self.work/'rebuilt-controls'))
        self.verify()
        for guard in self.vm_trees.values(): guard.check()
        self.captured.prepared=self
        return self.captured,self.work/'classes',self.work/'rebuilt',self.work/'rebuilt-controls',self.pinned


def prepare(directory,work,javac,root=ROOT,*,captured=None):
    javac=Path(javac).resolve()
    return PreparedRun(directory,root,work,[javac],captured=captured).compile(javac)


def run(directory=PACK, root=ROOT, java=None, javac=None, javap=None, openj9=None, work=None, *, single_primary=False):
    java = Path(java or shutil.which('java')).resolve()
    javac = Path(javac or shutil.which('javac')).resolve()
    javap = Path(javap or javac.with_name('javap.exe' if os.name == 'nt' else 'javap')).resolve()
    if type(single_primary) is not bool or (single_primary and openj9 is not None):
        raise ValueError('single-primary scope contradicts secondary runtime')
    if openj9 is None and not single_primary:
        raise ValueError('dual scope requires independent OpenJ9 runtime')
    runtime_scope='primary-only' if single_primary else 'dual'
    runtimes=[('zulu',java,[])]
    if not single_primary: runtimes.append(('openj9',Path(openj9).resolve(),['-Xshareclasses:none']))
    prepared=PreparedRun(directory,root,work,[java,javac,javap,*[vm for _,vm,_ in runtimes]])
    captured,_,_,_,pinned=prepared.compile(javac)
    command=prepared.command
    work=prepared.work
    compiler=prepared.compiler
    versions={v:version(command([p,'-version'])) for v,p,_ in runtimes}
    if 'OpenJ9' in versions['zulu'] or (not single_primary and 'OpenJ9' not in versions['openj9']): raise ValueError('expected primary HotSpot and independent OpenJ9 when requested')
    disassemblies = {}
    for stage in ['classes', 'rebuilt']:
        outputs = {}
        for cls in ['operand.OperandGuest', 'app.OperandMain', 'app.OperandUncaught']:
            outputs[cls] = command([javap, *JAVAP_FLAGS, '-cp', work / stage, cls])['stdout']
        disassemblies[stage] = inspect_sites(outputs)
    oracles = json.loads(captured.blobs['message-oracles.json'])
    rows = []
    for vendor, vm, options in runtimes:
        args = [encode(v) for v in oracles[vendor]['messages']]
        for stage, guest, control in [('archived', 'classes', 'controls'), ('rebuilt', 'rebuilt', 'rebuilt-controls')]:
            for mode in ([], ['-Xint']):
                for case, expected in {**MAIN, **UNCAUGHT}.items():
                    tail = [case, *args] if case in MAIN else ['uncaught', case]
                    result = command([vm, *options, *LOCALE, *mode, '-Xverify:all', '-cp',
                                      str(work / control) + os.pathsep + str(work / guest), 'ObserveOperandMain', *tail])
                    if result['stdout'] != expected:
                        raise ValueError(f'{vendor}/{stage}/{mode}/{case} wrong oracle: {result}')
                    rows.append({**{k: result[k] for k in ['exit_code', 'stdout']}, 'vm': vendor,
                                 'input': stage, 'mode': mode, 'case': case})
    validate_rows(rows,runtime_scope=runtime_scope)
    codecs = []
    for vendor, vm, options in runtimes:
        for token in CODECS:
            result = command([vm, *options, *LOCALE, '-Xverify:all', '-cp',
                              str(work / 'controls') + os.pathsep + str(work / 'classes'),
                              'ObserveOperandMain', 'decode', token], allow_failure=True)
            codecs.append({k: result[k] for k in ['exit_code', 'stdout', 'stderr']} | {'vm': vendor, 'token': token})
    validate_codecs(codecs,runtime_scope=runtime_scope)
    prepared.verify()
    for guard in prepared.vm_trees.values():
        guard.check()
    result = {'runtime_scope':runtime_scope,'required_runtimes':list(scope_runtimes(runtime_scope)),
              'secondary_requested':not single_primary,
              'expected_observations':44*len(runtimes),'expected_codec_checks':12*len(runtimes),
              'runtime_binaries':{name:{'path':str(vm),'sha256':digest(vm.read_bytes()),'version':versions[name]} for name,vm,_ in runtimes},
              'runs': rows, 'codecs': codecs, 'observations': len(rows), 'codec_checks': len(codecs),
              'compiler_exact_rebuild': pinned, 'compiler': compiler, 'versions': versions,
              'locale_flags': LOCALE, 'commands': prepared.commands, 'disassembly': disassemblies,
              'runtime_tool_trees': {p: g.expected for p, g in prepared.vm_trees.items()},
              'staged_tree_hashes': {str(g.directory.relative_to(work)): g.expected for g in prepared.guards if isinstance(g, FixedTree)},
              'compiler_binary_sha256': digest(javac.read_bytes()),
              'captured_input_hashes': {n: digest(b) for n, b in captured.blobs.items()},
              'tool_hashes': {str(p): h for p, h in captured.tools.items()}, 'posthashes_verified': True,
              'recording_written': False}
    validate_report(result,expected_scope=runtime_scope)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--pack', type=Path, default=PACK)
    parser.add_argument('--java', type=Path)
    parser.add_argument('--javac', type=Path)
    parser.add_argument('--javap', type=Path)
    scope=parser.add_mutually_exclusive_group(required=True)
    scope.add_argument('--compare-openj9',type=Path)
    scope.add_argument('--single-primary',action='store_true',help='explicit44behavior+12codec HotSpot-only scope; secondary not tested')
    parser.add_argument('--work-dir', type=Path)
    args = parser.parse_args()
    if args.work_dir:
        result = run(args.pack, ROOT, args.java, args.javac, args.javap, args.compare_openj9, args.work_dir,single_primary=args.single_primary)
    else:
        scratch = ROOT / 'target/fixture-tool'
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='operand-entry-', dir=scratch) as temp:
            result = run(args.pack, ROOT, args.java, args.javac, args.javap, args.compare_openj9, Path(temp) / 'run',single_primary=args.single_primary)
    print(json.dumps(result, indent=2, ensure_ascii=True))


if __name__ == '__main__':
    main()
