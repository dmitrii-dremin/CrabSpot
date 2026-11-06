"""Replay fixed independent operand oracles on CrabSpot's public API and actual CLI.

No product output is used to invent message, counter, or resource expectations.
Each subprocess is bounded separately; the full sweep intentionally has no 600s
outer timeout. All watched inputs are checked before AND after every subprocess.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import stat
import sys
import tempfile

import operand_entry_reference as reference
from verify_throwable_entry import stage
from verify_application_entry import extract_classes
from verify_java_base import pack_archive

ROOT = Path(__file__).resolve().parents[1]
MAIN = [184, 42, 6, 8, 8, 248, 29, 1]
COUNTERS = ('proved', 'witnesses', 'admitted', 'argv', 'arguments', 'evaluations',
            'discovery', 'throwable_discovery', 'diag_work', 'message_units', 'messages',
            'string_work', 'string_objects', 'string_units', 'snapshots', 'frames',
            'backtraces', 'trace_work', 'started', 'produced')
NAMES = ('java/lang/IndexOutOfBoundsException','java/lang/ArrayIndexOutOfBoundsException',
         'java/lang/NegativeArraySizeException','java/lang/ArrayStoreException','java/lang/ClassCastException')
WRONG = 'MainEvaluation uncaught app/OperandMain$Failure at app/OperandMain.require(Z)V@8'
UNCAUGHT = {
    'bounds': ('java/lang/ArrayIndexOutOfBoundsException','loadI([II)I@7'),
    'negative': ('java/lang/NegativeArraySizeException','multiI(III)[[[I@8'),
    'store': ('java/lang/ArrayStoreException','storeA([Ljava/lang/Object;ILjava/lang/Object;)V@8'),
    'cast': ('java/lang/ClassCastException','castInt(Ljava/lang/Object;)Ljava/lang/Object;@6'),
}

def digest(raw):
    return hashlib.sha256(raw).hexdigest()

def units(text):
    return len(text.encode('utf-16-be', 'surrogatepass')) // 2

def canonical_tool(path):
    raw=Path(path).read_bytes().replace(b'\r\n',b'\n')
    if b'\r' in raw:
        raise ValueError('bare CR in tool')
    return digest(raw)

def parse_output(output):
    rows=output.splitlines()
    if len(rows) not in (2,3):
        raise ValueError('invalid public row count')
    pairs=[x.partition('=') for x in rows[-1].split()]
    if tuple(x[0] for x in pairs)!=COUNTERS or any(mark!='=' or not n.isascii() or not n.isdecimal() or int(n)>2**64-1 for _,mark,n in pairs):
        raise ValueError('invalid public counters')
    usage={k:int(n) for k,_,n in pairs}
    fields=None
    if len(rows)==3:
        if not rows[1].startswith('fields='):
            raise ValueError('missing fields')
        fields=json.loads(rows[1][7:])
        if not isinstance(fields,list) or any(type(x)is not int for x in fields):
            raise ValueError('invalid fields')
        if not usage['proved'] or usage['proved']!=usage['witnesses'] or usage['proved']!=usage['admitted']:
            raise ValueError('proof/witness/layout mismatch')
        if usage['argv']!=1 or usage['evaluations']!=1:
            raise ValueError('not one argument array/evaluation')
    elif not rows[0].startswith('image ') or any(usage[k] for k in ('evaluations','started','produced')):
        raise ValueError('image error acquired executable effects')
    if usage['produced']>usage['started'] or usage['backtraces']>usage['snapshots'] or usage['messages']>usage['started']:
        raise ValueError('inconsistent completed counters')
    return {'status':rows[0],'fields':fields,'usage':usage}

def expect(row,status,fields,**counts):
    if row['status']!=status or row['fields']!=fields or any(row['usage'][k]!=v for k,v in counts.items()):
        raise ValueError(f'expected {status!r}/{fields}/{counts}, observed {row}')
    return row

def fixed_oracles(messages):
    """Counts from the Java loops and frozen diagnostic prices, not observed usage."""
    if len(messages)!=17 or any(x is None for x in messages):
        raise ValueError('unexpected CrabSpot fixed diagnostic policy')
    expected=[f'Index {i} out of bounds for length {n}' for i,n in [(-1,2),(2,2),(-2147483648,2),(2147483647,2),(0,0)]]
    expected+=['-1','-2147483648','-3','-4','java.lang.Object','[I','operand.\u00c9l\u00e9ment',
               'class [J cannot be cast to class [I','class [I cannot be cast to class [Ljava.lang.String;',
               'class java.lang.Object cannot be cast to class java.lang.String',
               'class operand.\u00c9l\u00e9ment cannot be cast to class operand.\u540d','/ by zero']
    if messages!=expected:
        raise ValueError('fixed independent message oracle changed')
    count=[38,36,36,36,38,18,18,2,4,2,2,2,2,2,2,2]
    lengths=list(map(units,messages[:16]))
    work=[]
    for i,L in enumerate(lengths):
        if i<5:
            index=[-1,2,-2147483648,2147483647,0][i]
            length=0 if i==4 else 2
            work.append(2+units(str(index))+units(str(length))+L) # Spec, base, decimal, emission
        elif i<9:
            work.append(2+L+L) # Spec, base, signed decimal, emission
        elif i<12:
            work.append((5 if i==10 else 8)+L) # primitive array source vs nominal; String component
        else:
            work.append([4,7,26,10][i-12]+L) # type structures and exact equal nominal comparison
    return {'messages':240,'message_units':sum(n*L for n,L in zip(count,lengths)),
            'diag_work':sum(n*w for n,w in zip(count,work)),
            'units':max(lengths),'temporary':2*max(lengths),
            # VM frames: bounds730 + negative162 + store18 + cast24 + arithmetic16.
            # Refills: 240*3+8*2=736. Each snapshot pays begin/finish/commit3;
            # each real refill wrapper's authenticated fill name costs18.
            'snapshots':496,'frames':1686,'backtraces':496,'trace_work':1686+496*3+248*18,
            'started':248,'produced':248}


def event_tape():
    bounds=[slot for _kind in range(9) for _store in range(2) for slot in range(5) for _repeat in range(2)]+[4,0,4,0]
    negative=[slot for _kind in range(9) for slot in (5,6) for _repeat in range(2)]+[7,7,8,8,8,8]
    return bounds+negative+[9,9,10,10,11,11]+[12,12,13,13,14,14,15,15]+[16]*8

def diagnostic_prefix(messages,key,limit):
    """Independent frozen-price simulation through the first rejected message."""
    tape=event_tape();work=total=done=0
    for index,slot in enumerate(tape):
        if slot==16:continue
        text=messages[slot];L=units(text)
        if slot<5:
            n=[-1,2,-2147483648,2147483647,0][slot];length=0 if slot==4 else 2;price=2+units(str(n))+units(str(length))+L
        elif slot<9:price=2+2*L
        elif slot<12:price=(5 if slot==10 else 8)+L
        else:price=[4,7,26,10][slot-12]+L
        if key=='diag-work' and work+1>limit:break
        work+=price-L # Spec and fixed pre-reservation message work
        required={'diag-work':work+L,'units':L,'total-units':total+L,'temporary':2*L}[key]
        if required>limit:break
        work+=L;total+=L;done+=1
    else:raise ValueError('limit does not reject a message')
    prior=tape[:index]
    fields=[sum(0<=x<5 for x in prior),sum(5<=x<9 for x in prior),sum(9<=x<12 for x in prior),sum(12<=x<16 for x in prior),sum(x==16 for x in prior),index,2*(index//20)if index<180 else 18 if index<240 else 29,0]
    return dict(fields=fields,diag_work=work,message_units=total,messages=done,started=index+1,produced=index)

class ClassMeta:
    """Small independent bounded reader for discovery-price inputs, never execution."""
    def __init__(self,raw):
        self.raw=raw;self.at=0
        if self.take(4)!=bytes.fromhex('cafebabe') or len(raw)>16<<20:
            raise ValueError('invalid class')
        self.take(4);count=self.u2();self.cp={};i=1
        while i<count:
            tag=self.take(1)[0]
            if tag==1:self.cp[i]=(tag,self.take(self.u2()))
            elif tag in (7,8,16,19,20):self.cp[i]=(tag,self.u2())
            elif tag in (5,6):self.take(8);i+=1
            elif tag in (3,4,9,10,11,12,17,18):self.take(4)
            elif tag==15:self.take(3)
            else:raise ValueError('invalid CP tag')
            i+=1
        self.flags=self.u2();self.name=self.class_name(self.u2());self.parent=self.class_name(self.u2())
        self.take(2*self.u2());self.members(False);self.methods=self.members(True)
    def take(self,n):
        if n<0 or self.at+n>len(self.raw):raise ValueError('truncated class')
        b=self.raw[self.at:self.at+n];self.at+=n;return b
    def u2(self):return int.from_bytes(self.take(2),'big')
    def utf(self,index):
        tag,value=self.cp[index]
        if tag!=1:raise ValueError('wrong Utf8 tag')
        return value.decode('utf8','surrogatepass')
    def class_name(self,index):
        if index==0:return None
        tag,value=self.cp[index]
        if tag!=7:raise ValueError('wrong Class tag')
        return self.utf(value)
    def members(self,keep):
        result=[]
        for _ in range(self.u2()):
            flags,name,desc=self.u2(),self.utf(self.u2()),self.utf(self.u2())
            for _ in range(self.u2()):self.take(2);self.take(int.from_bytes(self.take(4),'big'))
            if keep:result.append((flags,name,desc))
        return result

def discovery_work(blobs):
    """Registry prefix in declared UTF16-relative-path order; five fixed role passes."""
    found={};work=0
    for path in sorted(blobs,key=lambda x:x.encode('utf-16-be','surrogatepass')):
        if not path.endswith('.class') or path=='module-info.class':continue
        c=ClassMeta(blobs[path]);work+=3 # row1, self Class/Utf82
        for wanted in NAMES:
            if wanted not in found and units(c.name)==len(wanted):
                work+=len(wanted)
                if c.name==wanted:found[wanted]=c;break
        if len(found)==5:break
    if len(found)!=5:raise ValueError('required discovery roles missing')
    for i,name in enumerate(NAMES):
        c=found[name];parent=NAMES[0] if i==1 else 'java/lang/RuntimeException'
        if c.parent!=parent or c.flags&0x8600 or not c.flags&1:raise ValueError('invalid discovery role')
        work+=3+len(parent) # revisit1, parent Class/Utf82, exact name
        for flags,n,d in c.methods:
            work+=2 # member and name Utf8
            if units(n)==6:work+=6
            if n!='<init>':continue
            work+=1
            if units(d)==len('(Ljava/lang/String;)V'):work+=len('(Ljava/lang/String;)V')
            if d=='(Ljava/lang/String;)V':
                if flags&7!=1 or flags&(8|0x100|0x400):raise ValueError('invalid constructor')
                break
        else:raise ValueError('String constructor absent')
    return work

class Runner:
    def __init__(self,check,report=None):
        self.check=check;self.rows=[];self.report=report
    def run(self,command,code=0,diagnostic=None,validator=None):
        self.check()
        print(f'[{len(self.rows)+1}] {command[0]} {command[1:4]}',flush=True)
        try:result=subprocess.run([str(x) for x in command],capture_output=True,timeout=240)
        finally:self.check()
        out,err=result.stdout.decode('utf8'),result.stderr.decode('utf8')
        required=() if diagnostic is None else (diagnostic,) if isinstance(diagnostic,str) else diagnostic
        self.rows.append({'command':list(map(str,command)),'exit':result.returncode,'stdout':out,'stderr':err,'accepted':False})
        def save():
            if self.report:
                self.report.parent.mkdir(parents=True,exist_ok=True)
                self.report.write_bytes((json.dumps(self.rows,indent=2)+'\n').encode('utf8'))
        save()
        if result.returncode!=code or (diagnostic is None and err) or any(x not in err for x in required):
            raise ValueError(f'command failed exit={result.returncode}: {command!r}\n{out}\n{err}')
        if validator:validator(err)
        self.rows[-1]['accepted']=True;save()
        return out

def validate_null_cli(stderr):
    pattern=(r'crabspot: MainEvaluation: owner ClassId \{ registry: (?P<registry>[0-9]+), index: (?P<owner>[0-9]+) \} '
             r'method #[0-9]+ loadI\(\[II\)I: caller owner ClassId \{ registry: (?P=registry), index: (?P<caller>[0-9]+) \} '
             r'call site method #[0-9]+ at bytecode offset 112: bytecode offset 7: '
             r'VM exception production is unsupported: NullPointerException\r?\n?')
    match=re.fullmatch(pattern,stderr)
    if not match or match['owner']==match['caller']:raise ValueError('wrong unsupported NPE class/fault/caller provenance')

def verify_directories(watched):
    for directory,expected in watched:
        if snapshot_hashes(directory)!=expected:raise ValueError('captured input changed: '+str(directory))


def checked_files(directory,max_files=20000,max_bytes=134217728):
    directory=Path(directory);count=total=directories=0
    def regular(path,want_dir):
        info=path.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info,'st_file_attributes',0)&0x400:raise ValueError('linked/reparse input')
        if not (stat.S_ISDIR(info.st_mode)if want_dir else stat.S_ISREG(info.st_mode)):raise ValueError('special input')
        return info
    regular(directory,True)
    for parent,dirs,files in os.walk(directory,followlinks=False):
        directories+=1
        if directories>20000:raise ValueError('directory inventory limit')
        for name in dirs:regular(Path(parent)/name,True)
        for name in sorted(files):
            path=Path(parent)/name;info=regular(path,False);count+=1
            if count>max_files or info.st_size>max_bytes-total or info.st_size>16<<20:raise ValueError('file snapshot limit')
            total+=info.st_size
            yield path,path.relative_to(directory).as_posix(),info

def snapshot_directory(directory):
    blobs={}
    for path,name,info in checked_files(directory):
        # Reserve/read only this prepaid file's actual bounded size, never the
        # remaining aggregate128MiB for each small class.
        with path.open('rb')as source:data=source.read(info.st_size+1)
        if len(data)!=info.st_size:raise ValueError('snapshot size changed')
        blobs[name]=data
    if 'java/lang/Object.class'not in blobs or 'java/lang/Throwable.class'not in blobs:raise ValueError('incomplete platform')
    return blobs

def simple_hashes(directory):
    result={}
    for path,name,info in checked_files(directory):
        h=hashlib.sha256();count=0
        with path.open('rb')as source:
            while True:
                chunk=source.read(65536)
                if not chunk:break
                count+=len(chunk)
                if count>info.st_size:raise ValueError('hashed input grew')
                h.update(chunk)
        if count!=info.st_size:raise ValueError('hashed input size changed')
        result[name]=h.hexdigest()
    return result

def snapshot_hashes(directory):
    result=simple_hashes(directory)
    if 'java/lang/Object.class'not in result or 'java/lang/Throwable.class'not in result:raise ValueError('incomplete platform')
    return result

def suite(runner,public,cli,platform,classes,arguments,oracle,messages,full=True):
    rows=[]
    def call(entry='OperandMain',options=(),argv=None):
        row=parse_output(runner.run([public,platform,classes,'app.'+entry,'field=16777216',*options,'--',*(arguments if argv is None else argv)]));rows.append(row);return row
    base={k:oracle[k] for k in ('messages','message_units','diag_work','snapshots','frames','backtraces','trace_work','started','produced')}
    base['discovery']=oracle['discovery']
    expect(call(),'returned',MAIN,**base)
    default=call(options=('field=1000000',))
    if not default['status'].endswith('FieldLookupSteps 1000000/1000001') or default['fields'][-1]!=0:raise ValueError('default Field ceiling did not retain real quota failure')
    expect(call(options=('strict',)),'returned',MAIN,**base)
    expect(call(options=('ordinary',)),'returned',MAIN,**base)
    for route in ('old','disabled'):
        expect(call(options=(route,)),'image Dependencies UnsupportedPlatformReference',None,discovery=0,started=0,produced=0,arguments=0,evaluations=0)
    for slot,fields,started in [(0,[0]*8,1),(9,[184,42,0,0,0,226,18,0],227),(12,[184,42,6,0,0,232,18,0],233),(16,[184,42,6,8,0,240,29,0],241)]:
        wrong=list(arguments);wrong[slot*2+1]+='!'
        expect(call(argv=wrong),WRONG,fields,started=started,produced=started)
    for case,(kind,head) in UNCAUGHT.items():
        expect(call('OperandUncaught',argv=[case]),f'MainEvaluation uncaught {kind} at operand/OperandGuest.{head}',[7,0],started=1,produced=1,messages=1)
    expect(call('OperandUncaught',argv=['refilled']),'MainEvaluation uncaught java/lang/ArrayIndexOutOfBoundsException at app/OperandUncaught.main([Ljava/lang/String;)V@143',[7,0],started=1,produced=1)
    null=call('OperandUncaught',argv=['null'])
    if null['fields']!=[7,0] or 'UnsupportedVmException NullPointerException' not in null['status'] or not null['status'].startswith('MainEvaluation failed loadI([II)I@7 '):raise ValueError('null boundary was misrepresented as catchable')
    if any(null['usage'][k] for k in ('started','produced','messages','diag_work')):raise ValueError('null boundary gained materialization')
    runner.run([cli,'run','--field-lookup-steps','16777216','--platform',platform,'--classes',classes,'app.OperandMain','--',*arguments])
    wrong=list(arguments);wrong[1]+='!'
    runner.run([cli,'run','--field-lookup-steps','16777216','--platform',platform,'--classes',classes,'app.OperandMain','--',*wrong],1,'uncaught app/OperandMain$Failure at app/OperandMain.require(Z)V@8')
    for case,(kind,head) in UNCAUGHT.items():runner.run([cli,'run','--field-lookup-steps','16777216','--platform',platform,'--classes',classes,'app.OperandUncaught','--',case],1,f'uncaught {kind} at operand/OperandGuest.{head}')
    runner.run([cli,'run','--field-lookup-steps','16777216','--platform',platform,'--classes',classes,'app.OperandUncaught','--','refilled'],1,'app/OperandUncaught.main([Ljava/lang/String;)V@143')
    runner.run([cli,'run','--field-lookup-steps','16777216','--platform',platform,'--classes',classes,'app.OperandUncaught','--','null'],1,(),validator=validate_null_cli)
    if not full:return rows
    expect(call(options=(f'discovery={oracle["discovery"]}',)),'returned',MAIN,**base)
    for limit in (0,oracle['discovery']-1):
        r=call(options=(f'discovery={limit}',))
        if not re.fullmatch(rf'image Verification DiscoveryWork {limit}/\d+',r['status']) or r['fields'] is not None or any(r['usage'][k] for k in ('proved','witnesses','admitted','argv','arguments','evaluations','started','produced')):raise ValueError('discovery preflight acquired effects')
    for key in ('diag-work','units','total-units','temporary'):
        cap=oracle[{'diag-work':'diag_work','units':'units','total-units':'message_units','temporary':'temporary'}[key]]
        expect(call(options=(f'{key}={cap}',)),'returned',MAIN,**base)
        for limit in (0,cap-1):
            r=call(options=(f'{key}={limit}',))
            resource={'diag-work':'DiagnosticWork','units':'MessageUnits','total-units':'TotalMessageUnits','temporary':'TemporaryBytes'}[key]
            if not re.search(rf' {resource} {limit}/\d+$',r['status']) or not r['status'].startswith('MainEvaluation failed '):raise ValueError('wrong diagnostic limit')
            prefix=diagnostic_prefix(messages,key,limit)
            if r['fields']!=prefix.pop('fields') or any(r['usage'][k]!=v for k,v in prefix.items()):raise ValueError('diagnostic failure lost independently counted paid prefix: '+str(r))
    for key,cap,resource in [('vm',248,'VmExceptionLimit'),('snapshots',496,'Snapshots'),('trace-frames',1686,'Frames'),('trace-work',oracle['trace_work'],'TraceWork'),('depth',4,'FramesPerSnapshot')]:
        expect(call(options=(f'{key}={cap}',)),'returned',MAIN,**base)
        for limit in (0,cap-1):
            r=call(options=(f'{key}={limit}',))
            if not re.search(rf' {resource} {limit}/\d+$',r['status']):raise ValueError('wrong producer/trace limit')
            if key=='vm':
                expected=[184,42,6,8,7,247,29,0] if limit else [0]*8
                if r['fields']!=expected or r['usage']['started']!=limit or r['usage']['produced']!=limit or (not limit and r['usage']['diag_work']):raise ValueError('VM admission lost independent prefix')
    runner.run([cli,'run','--platform',platform,'--classes',classes,'app.OperandMain','--',*arguments],1,'field LookupSteps limit 1000000 exceeded (required 1000001)')
    # First bounds event's public minimum is fixed by -1 and length2.
    for opts,success in [(('diag-work=40','units=35','total-units=35','temporary=70'),True),(('diag-work=39',),False)]:
        r=call('OperandUncaught',options=opts,argv=['bounds'])
        if success:expect(r,'MainEvaluation uncaught java/lang/ArrayIndexOutOfBoundsException at operand/OperandGuest.loadI([II)I@7',[7,0],diag_work=40,message_units=35,messages=1)
        elif r['usage']['diag_work']!=5 or r['usage']['message_units'] or r['usage']['produced']:raise ValueError('one-short first bound did not preserve accepted work5')
    # Independent setup thresholds:34 fresh strings, exact fixed argument units.
    argunits=sum(map(units,arguments));argwork=argunits+34
    for option,label in [('string-objects=0','StringObjects 0/1'),('string-objects=33','StringObjects 33/34'),('string-units=0','StringRetainedUnits'),(f'string-units={argunits-1}','StringRetainedUnits'),('string-work=0','StringWorkUnits'),(f'string-work={argwork-1}','StringWorkUnits'),('arrays=0','HeapArrays 0/1'),('array-length=33','HeapArrayLength 33/34')]:
        r=call(options=(option,))
        if not r['status'].startswith('image Setup ') or label not in r['status'] or r['fields'] is not None or r['usage']['evaluations'] or r['usage']['started']:raise ValueError('setup resource failure changed stage/effects')
    for option in ('string-objects=34',f'string-units={argunits}',f'string-work={argwork}'):
        r=call(options=(option,))
        if not r['status'].startswith('MainEvaluation failed ') or r['fields']!=[0]*8 or r['usage']['arguments']!=34 or r['usage']['evaluations']!=1 or r['usage']['started']!=1 or r['usage']['produced'] or r['usage']['diag_work']!=1 or r['usage']['message_units'] or r['usage']['messages'] or ':Initialization:' not in r['status']:raise ValueError('exact argument String ceiling did not retain first class-initialization prefix')
    return rows

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--jdk',type=Path,default=os.environ.get('JAVA_HOME'))
    parser.add_argument('--platform',type=Path)
    parser.add_argument('--pack',type=Path,default=reference.PACK)
    parser.add_argument('--bin-dir',type=Path)
    parser.add_argument('--work-dir',type=Path)
    parser.add_argument('--report',type=Path)
    parser.add_argument('--smoke',action='store_true')
    args=parser.parse_args()
    if not args.jdk:parser.error('provide --jdk or JAVA_HOME')
    suffix='.exe' if os.name=='nt' else '';java,javac=(args.jdk/'bin'/(n+suffix)for n in ('java','javac'))
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target'))).resolve()
    parent=args.work_dir or target/'operand-entry-check';parent.mkdir(parents=True,exist_ok=True)
    # Capture all fixture buffers BEFORE javac/version/build subprocesses.
    def phase(name):
        print('phase '+name,flush=True)
        if args.report:
            args.report.parent.mkdir(parents=True,exist_ok=True)
            args.report.with_suffix('.phase.json').write_bytes((json.dumps({'phase':name})+'\n').encode('utf8'))
    phase('capture')
    captured=reference.capture(args.pack)
    source_paths={Path(__file__).resolve(),ROOT/'crates/launcher/examples/replay_operand_entry.rs',ROOT/'tools/test_operand_launch.py'}
    for module in tuple(sys.modules.values()):
        name=getattr(module,'__file__',None)
        if name:
            p=Path(name).resolve()
            if p.suffix=='.py' and ROOT/'tools' in p.parents:source_paths.add(p)
    source_pins={str(p):canonical_tool(p) for p in source_paths};tool_pins={str(p):digest(p.read_bytes())for p in (java,javac,Path(sys.executable))}
    with tempfile.TemporaryDirectory(prefix='public-',dir=parent) as tmp:
        work=Path(tmp)
        # prepare consumes its own pre-subprocess capture and enforces exact inventory.
        phase('compile captured fixtures')
        prepared,original,rebuilt,controls,pinned=reference.prepare(args.pack,work/'fixture',javac,captured=captured)
        if prepared.blobs!=captured.blobs:raise ValueError('fixture changed before preparation')
        arguments=reference.product_arguments(captured)
        messages=json.loads(captured.blobs['message-oracles.json'])['crabspot']['messages'];oracle=fixed_oracles(messages)
        supplied=args.platform
        if supplied is None:
            supplied=work/'extracted';archive=work/'packed';_,count,_=pack_archive(args.jdk/'jmods/java.base.jmod',archive)
            if extract_classes(archive,supplied)!=count+1:raise ValueError('incomplete extracted platform')
        phase('capture platform')
        blobs=snapshot_directory(supplied);platform=stage(blobs,work/'platform');ph={k:digest(v)for k,v in blobs.items()};oracle['discovery']=discovery_work(blobs)
        phase('platform captured and independent discovery counted')
        if not args.bin_dir:
            subprocess.run(['cargo','build','--locked','--release','-j1','-p','crabspot-launcher','--bin','crabspot','--example','replay_operand_entry'],cwd=ROOT,check=True)
        bins=args.bin_dir or target/'release';public=bins/'examples'/('replay_operand_entry'+suffix);cli=bins/('crabspot'+suffix)
        bp={str(p):digest(p.read_bytes())for p in (public,cli)}
        watched=[(p,simple_hashes(p))for p in (original,rebuilt,controls,work/'fixture/source',work/'fixture/control')]
        def check():
            captured.check();prepared.check();prepared.prepared.verify()
            for p,h in watched:
                if simple_hashes(p)!=h:raise ValueError('staged input changed: '+str(p))
            if snapshot_hashes(platform)!=ph:raise ValueError('staged platform changed')
            for p,h in {**bp,**tool_pins}.items():
                if digest(Path(p).read_bytes())!=h:raise ValueError('binary/tool changed: '+p)
            for p,h in source_pins.items():
                if canonical_tool(p)!=h:raise ValueError('harness changed: '+p)
        def original_check():
            if snapshot_hashes(supplied)!=ph:raise ValueError('original supplied platform changed')
            for tree in prepared.prepared.vm_trees.values():tree.check()
        original_check()
        runner=Runner(check, args.report.with_suffix('.progress.json') if args.report else None)
        report={'oracle':oracle,'source_hashes':source_pins,'tool_hashes':tool_pins,'binary_hashes':bp,'platform_hashes':ph,'compiler_exact_rebuild':pinned,'compiler_version':prepared.prepared.compiler,'python':sys.version,'runtime_tree_hashes':{p:g.expected for p,g in prepared.prepared.vm_trees.items()},'inputs':{},'product':{}}
        for label,classes in [('archived',original),('rebuilt',rebuilt)]:
            report['inputs'][label]=simple_hashes(classes);report['product'][label]=suite(runner,public,cli,platform,classes,arguments,oracle,messages,not args.smoke)
        check();original_check();report['original_runtime_checks']='pre/post overall; staged/tool/binary guards before/after every product child';report['commands']=runner.rows;report['inter_subprocess_rechecks']=True
        if args.report:args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_bytes((json.dumps(report,indent=2)+'\n').encode('utf8'))
        print(json.dumps({'commands':len(runner.rows),'oracle':oracle,'compiler_exact_rebuild':pinned},indent=2))

if __name__=='__main__':main()
