#!/usr/bin/env python3
"""Qualify synthetic copies through the five real, already migrated desktops.

This writes labeled test clipboard contents in the logged-in X11 sessions.
It never replaces configuration, credentials or the running agent. Private
receiver/browser profiles are separate; reports contain only fixture metadata.
Native application paste is established; physical keyboard acceptance is not.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import shlex
import subprocess
import time
import uuid


HOSTS = {'gbs-p2': 'gbs-p2', 'gbs-p3': 'gbs-p3', 'asus': 'gbs-asus-vpn',
         'acer': 'gbs-acer-vpn', 'zaza-desktop': None}

HELPER = r'''
import hashlib,json,os,pathlib,pwd,signal,struct,subprocess,sys,time,tomllib,urllib.request,zlib
r=json.load(sys.stdin);op=r['op'];account=pwd.getpwnam('zaza')
home=pathlib.Path(account.pw_dir);root=home/'.local/state/poolsync/native-qualification'/r['run']
cfg_path=home/'.config/poolsync/agent.toml';cfg=tomllib.loads(cfg_path.read_text())
def digest(path):
 with pathlib.Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def env():return json.loads((root/'environment.json').read_text())
def spawn(args,log):
 with (root/log).open('ab') as stream:
  p=subprocess.Popen(args,env=env(),cwd=root,stdout=stream,stderr=stream,user=account.pw_uid,group=account.pw_gid,extra_groups=os.getgrouplist('zaza',account.pw_gid),start_new_session=True)
 tracking=root/'workers.json'
 pids=json.loads(tracking.read_text()) if tracking.exists() else []
 tracking.write_text(json.dumps(pids+[p.pid]))
 return p.pid
def stop(pid):
 try:
  command=pathlib.Path('/proc',str(pid),'cmdline').read_bytes()
  if r['run'].encode() not in command:return
 except FileNotFoundError:return
 try:os.killpg(pid,signal.SIGTERM)
 except ProcessLookupError:pass
def native(args):
 return subprocess.run(args,env=env(),cwd=root,user=account.pw_uid,group=account.pw_gid,extra_groups=os.getgrouplist('zaza',account.pw_gid),capture_output=True,text=True,timeout=8)
def owner():
 # Query with the graphical account's cookie, as for the GTK copier/receiver.
 # Root's default Xauthority does not authorize an XRDP user's display.
 code="""import ctypes,os
l=ctypes.CDLL('libX11.so.6');l.XOpenDisplay.argtypes=[ctypes.c_char_p];l.XOpenDisplay.restype=ctypes.c_void_p;l.XInternAtom.argtypes=[ctypes.c_void_p,ctypes.c_char_p,ctypes.c_int];l.XInternAtom.restype=ctypes.c_ulong;l.XGetSelectionOwner.argtypes=[ctypes.c_void_p,ctypes.c_ulong];l.XGetSelectionOwner.restype=ctypes.c_ulong;l.XCloseDisplay.argtypes=[ctypes.c_void_p]
d=l.XOpenDisplay(os.environ['DISPLAY'].encode());assert d
try:print(l.XGetSelectionOwner(d,l.XInternAtom(d,b'CLIPBOARD',0)))
finally:l.XCloseDisplay(d)
"""
 result=native(['/usr/bin/python3','-c',code]);assert result.returncode==0,result.stderr
 return int(result.stdout)
if op=='prepare':
 assert cfg['node']==r['node'] and cfg.get('hubless') and cfg.get('peer_direct_clipboard') and not cfg.get('hub_clipboard',True)
 assert cfg.get('e2e_key') and not cfg_path.with_suffix('.away').exists(),'target is absent from the pool'
 assert cfg['mode']==('full' if cfg['node'] in ('asus','acer') else 'clipboard_only')
 agents=[]
 for p in pathlib.Path('/proc').iterdir():
  if not p.name.isdigit():continue
  try:
   a=(p/'cmdline').read_bytes().split(b'\0')
   if a and a[0].decode()==str(home/'.local/bin/poolsync-agent'):agents.append(p)
  except (OSError,UnicodeError):pass
 assert len(agents)==1,'expected one running production agent'
 agent=agents[0];assert digest(agent/'exe')==r['sha256'],'running executable differs'
 environment=dict(v.decode().split('=',1) for v in (agent/'environ').read_bytes().split(b'\0') if b'=' in v)
 root.mkdir(mode=0o700,parents=True);os.chown(root,account.pw_uid,account.pw_gid)
 (root/'environment.json').write_text(json.dumps(environment));(root/'environment.json').chmod(0o600)
 (root/'baseline.json').write_text(json.dumps({'pid':int(agent.name),'config_sha256':digest(cfg_path)}))
 for name in ('clipboard-receiver.py','browser-paste-server.py','clipboard-paste.html'):
  contents=r['files'][name]
  if name=='clipboard-paste.html':contents=contents.replace('<title>PoolSync clipboard paste test</title>','<title>PoolSync fleet paste test '+r['run']+'</title>')
  p=root/name;p.write_text(contents);os.chown(p,account.pw_uid,account.pw_gid)
 images={}
 for name,w,h in (('first',193,127),('second',83,59)):
  # A retained image from a prior campaign must not match this fresh fixture.
  # Native receivers intentionally suppress repeated identical pixel hashes.
  rgb=hashlib.sha256((r['run']+name).encode()).digest()[:3];rows=b''.join(b'\0'+rgb*w for _ in range(h))
  def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
  png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(rows))+chunk(b'IEND',b'')
  p=root/(name+'.png');p.write_bytes(png);os.chown(p,account.pw_uid,account.pw_gid)
  images[name]=hashlib.sha256((rgb+b'\xff')*(w*h)).hexdigest()
 (root/'images.json').write_text(json.dumps(images))
 ready=native(['/usr/bin/python3','-c','import gi;gi.require_version("Gtk","3.0");from gi.repository import Gtk;Gtk.init([])']);assert ready.returncode==0,'native GTK session unavailable'
 output={'node':cfg['node'],'pid':int(agent.name),'display':environment.get('DISPLAY'),'mode':cfg['mode'],'running_sha256':r['sha256'],'images':images}
elif op=='receiver':
 output=spawn(['/usr/bin/python3',str(root/'clipboard-receiver.py'),'--watch','--quit-after','1200','--log',str(root/'pastes.json')],'receiver.log')
elif op=='copy':
 previous=owner()
 code='import gi,sys;gi.require_version("Gtk","3.0");from gi.repository import Gtk,Gdk,GdkPixbuf;Gtk.init([]);c=Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD);c.set_text(sys.argv[2],-1) if sys.argv[1]=="text" else c.set_image(GdkPixbuf.Pixbuf.new_from_file(sys.argv[2]));Gtk.main()'
 argument=r['text'] if r.get('text') is not None else str(root/(r['image']+'.png'))
 pid=spawn(['/usr/bin/python3','-c',code,'text' if r.get('text') is not None else 'image',argument],'copy.log')
 until=time.monotonic()+5
 while not (owner() and owner()!=previous):
  assert time.monotonic()<until,'native copying application did not claim clipboard';time.sleep(.05)
 old=root/'copy-pid'
 if old.exists():stop(int(old.read_text()))
 old.write_text(str(pid));output=True
elif op=='records':
 try:output=json.loads((root/('browser-pastes.json' if r.get('browser') else 'pastes.json')).read_text())
 except (FileNotFoundError,json.JSONDecodeError):output=[]
elif op=='browser':
 profile=root/'browser-profile';profile.mkdir(mode=0o700);os.chown(profile,account.pw_uid,account.pw_gid)
 server=spawn(['/usr/bin/python3',str(root/'browser-paste-server.py'),'--html',str(root/'clipboard-paste.html'),'--log',str(root/'browser-pastes.json'),'--port','19680'],'browser-server.log')
 until=time.monotonic()+5
 while True:
  try:
   opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
   with opener.open('http://127.0.0.1:19680/',timeout=1) as response:page=response.read()
   assert ('PoolSync fleet paste test '+r['run']).encode() in page,'unexpected native receiver page'
   break
  except OSError:
   assert time.monotonic()<until,'native receiver HTTP server unavailable';time.sleep(.05)
 # Keep the private fixture on the verified X11 display and loopback page,
 # independent of the account's proxy, keyring and browser startup defaults.
 browser=spawn(['/usr/bin/google-chrome','--ozone-platform=x11','--no-proxy-server','--password-store=basic','--disable-background-networking','--no-first-run','--no-default-browser-check','--user-data-dir='+str(profile),'--new-window','http://127.0.0.1:19680/'],'browser.log')
 until=time.monotonic()+20
 while True:
  found=native(['/usr/bin/xdotool','search','--name','PoolSync fleet paste test '+r['run']])
  if found.returncode==0:break
  assert time.monotonic()<until,'native Chrome receiver window unavailable';time.sleep(.15)
 window=found.stdout.splitlines()[-1];(root/'browser-window').write_text(window)
 output={'server':server,'browser':browser}
elif op=='browser_paste':
 window=(root/'browser-window').read_text()
 # Clipboard-only nodes cannot claim KVM: asking would create denial toasts.
 # Return an active KVM grab locally only on an eligible full-mode desktop.
 if cfg['mode']=='full':
  command=native(['/usr/bin/xdotool','key','ctrl+alt+shift+m']);assert command.returncode==0
  time.sleep(.25)
 command=native(['/usr/bin/xdotool','windowraise',window,'windowfocus','--sync',window,'key','ctrl+v']);assert command.returncode==0
 output=True
elif op=='status':output=json.loads(cfg_path.with_suffix('.status.json').read_text())
elif op=='cleanup':
 tracking=root/'workers.json'
 for pid in set(r['pids']+(json.loads(tracking.read_text()) if tracking.exists() else [])):stop(pid)
 if (root/'copy-pid').exists():stop(int((root/'copy-pid').read_text()))
 baseline=json.loads((root/'baseline.json').read_text())
 output={'config_preserved':digest(cfg_path)==baseline['config_sha256'],'agent_pid_preserved':pathlib.Path('/proc',str(baseline['pid'])).exists(),'running_sha256_preserved':digest(pathlib.Path('/proc',str(baseline['pid']),'exe'))==r['sha256']}
else:raise ValueError('unknown operation')
print(json.dumps(output))
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--expected-sha256', required=True)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--rounds', type=int, default=15)
    parser.add_argument('--native-browser', action='store_true')
    parser.add_argument('--nodes', nargs='+', choices=list(HOSTS), default=list(HOSTS),
                        help='Explicit physical subset; omitting this qualifies all five computers')
    args = parser.parse_args()
    targets = {node: HOSTS[node] for node in args.nodes}
    assert len(targets) == len(args.nodes), 'duplicate physical target'
    run = uuid.uuid4().hex
    checks, latencies, workers, observations = [], [], {}, {}
    files = {p: Path(__file__).with_name(p).read_text() for p in ('clipboard-receiver.py', 'browser-paste-server.py', 'clipboard-paste.html')}
    result = {'candidate_sha256': args.expected_sha256, 'selected_nodes': list(targets), 'input_source': 'synthetic native GTK copy applications', 'checks': checks, 'paste_convergence_seconds': latencies}

    def operation(node, op, **values):
        request = {'run': run, 'node': node, 'op': op, 'sha256': args.expected_sha256, **values}
        command = ['ssh', 'root@'+HOSTS[node], 'python3 -c '+shlex.quote(HELPER)] if HOSTS[node] else ['sudo', '-n', 'python3', '-c', HELPER]
        response = subprocess.run(command, input=json.dumps(request), text=True, capture_output=True, timeout=30)
        assert response.returncode == 0, f'{node}: {op} failed: {response.stderr[-1500:]}'
        return json.loads(response.stdout)

    def parallel(op, **values):
        with ThreadPoolExecutor(max_workers=5) as executor:
            pending = {node: executor.submit(operation, node, op, **values) for node in targets}
            return {node: future.result() for node, future in pending.items()}

    def check(name):
        checks.append({'name': name, 'passed': True})
        print(json.dumps(checks[-1]), flush=True)

    def expect(hash_, since, browser=False):
        until = time.monotonic()+20
        while time.monotonic() < until:
            received = parallel('records', browser=browser)
            if all(any(p.get('sha256') == hash_ and p.get('at', 0) >= since-.2 for p in records) for records in received.values()):
                latencies.append(round(time.time()-since, 3))
                return
            time.sleep(.15)
        missing=[node for node, records in received.items()
                 if not any(p.get('sha256')==hash_ and p.get('at',0)>=since-.2 for p in records)]
        result['failed_paste']={'route':'native browser' if browser else 'native GTK',
                                'expected_sha256':hash_,'since':since,'missing_nodes':missing,
                                'observed_metadata':{node:records[-5:] for node,records in received.items()}}
        raise AssertionError('Native '+('browser' if browser else 'GTK')+' paste did not converge on '+', '.join(missing))

    try:
        observations.update(parallel('prepare', files=files))
        workers.update({node: [pid] for node, pid in parallel('receiver').items()})
        check('Exact candidate runs in the selected original graphical sessions with preserved modes')
        if args.native_browser:
            for node, pids in parallel('browser').items(): workers[node].extend(pids.values())
        time.sleep(1)
        for index in range(args.rounds):
            node = list(targets)[index % len(targets)]
            image = 'first' if index % 2 == 0 else 'second'
            since = time.time(); operation(node, 'copy', image=image)
            expect(observations[node]['images'][image], since)
            if args.native_browser:
                parallel('browser_paste'); expect(observations[node]['images'][image], since, browser=True)
            text = 'PoolSync native qualification '+run+' round '+str(index)+' from '+node
            since = time.time(); operation(node, 'copy', text=text)
            expect(hashlib.sha256(text.encode()).hexdigest(), since)
            if args.native_browser:
                parallel('browser_paste'); expect(hashlib.sha256(text.encode()).hexdigest(), since, browser=True)
            check('Native image/text round '+str(index)+' copied from '+node)
        result['native_sessions'] = observations
        result['peer_status'] = parallel('status')
        result['functional_checks_passed'] = True
    except Exception as error:
        result['functional_checks_passed'] = False
        result['error'] = str(error)
        print(json.dumps({'error': str(error)}), flush=True)
    finally:
        restored = {}
        for node in observations:
            try: restored[node] = operation(node, 'cleanup', pids=workers.get(node, []))
            except Exception as error: restored[node] = {'error': str(error)}
        result['preservation'] = restored
        result['preservation_passed'] = len(restored) == len(targets) and all(all(v is True for v in data.values()) for data in restored.values())
        args.output.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2)+'\n'); args.output.chmod(0o600)
    print(json.dumps({key: result.get(key) for key in ('functional_checks_passed', 'preservation_passed', 'error')}))
    return 0 if result.get('functional_checks_passed') and result.get('preservation_passed') else 1


if __name__ == '__main__': raise SystemExit(main())
