#!/usr/bin/env python3
"""Qualify an isolated no-hub mesh on three disposable Podman X11 desktops.

Run inside the test host, with root access to Podman. Existing desktop agents
remain running and are verified in finally; their configuration is never
rewritten. Candidate Xorg display, HOME, credentials, locks and history are isolated.
The configured hub is loopback port 1 in each desktop, checked unreachable.
This tests agent restarts, not full container reboots or physical input devices.
"""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import secrets
import subprocess
import time
import uuid


XORG_CONFIG = '''Section "ServerFlags"
  Option "DontVTSwitch" "true"
  Option "AllowMouseOpenFail" "true"
  Option "AutoEnableDevices" "false"
  Option "AutoAddDevices" "false"
EndSection
Section "Device"
  Identifier "test-video"
  Driver "dummy"
  VideoRam 256000
EndSection
Section "Monitor"
  Identifier "test-monitor"
  HorizSync 5.0 - 1000.0
  VertRefresh 5.0 - 200.0
  Modeline "1600x900-test" 45.75 1600 1648 1800 2000 900 903 908 916 -Hsync +Vsync
EndSection
Section "Screen"
  Identifier "test-screen"
  Device "test-video"
  Monitor "test-monitor"
  DefaultDepth 24
  SubSection "Display"
    Depth 24
    Modes "1600x900-test"
  EndSubSection
EndSection
'''


HELPER = r'''
import base64, hashlib, json, os, signal, socket, subprocess, sys, time
from pathlib import Path
request=json.load(sys.stdin);op=request['op'];root=Path(request['root'])
def digest(path):
    try:
        with open(path,'rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
    except PermissionError:
        code="import hashlib,sys;print(hashlib.file_digest(open(sys.argv[1],'rb'),'sha256').hexdigest())"
        r=user(['python3','-c',code,str(path)]);assert r.returncode==0
        return r.stdout.decode().strip()
def run(args,**kwargs):
    return subprocess.run(args,capture_output=True,timeout=5,**kwargs)
def user(args,**kwargs):
    return run(['runuser','-m','-u','zaza','--',*args],**kwargs)
def env():return json.loads((root/'environment.json').read_text())
def spawn(args,log):
    # Keep a parent waiting on the worker so container PID 1 cannot leave an
    # unreaped agent PID that its existing single-instance guard sees as alive.
    pid_file=root/(log+'.pid.'+str(os.getpid()))
    guardian="import os,subprocess,sys;from pathlib import Path;p=subprocess.Popen(sys.argv[2:],user='zaza',group='zaza',extra_groups=[],start_new_session=True);Path(sys.argv[1]).write_text(str(p.pid));p.wait()"
    with (root/log).open('ab') as stream:
        process=subprocess.Popen(['python3','-c',guardian,str(pid_file),*args],env=env(),stdout=stream,stderr=stream,start_new_session=True)
    for _ in range(40):
        if pid_file.exists() and pid_file.read_text().strip():return int(pid_file.read_text())
        time.sleep(.05)
    raise RuntimeError('worker did not start')
def stop(pid):
    try:os.killpg(pid,signal.SIGTERM)
    except ProcessLookupError:return
    for _ in range(30):
        try:os.kill(pid,0)
        except ProcessLookupError:return
        time.sleep(.05)
    try:os.killpg(pid,signal.SIGKILL)
    except ProcessLookupError:pass
def clipboard_owner():
    import ctypes
    lib=ctypes.CDLL('libX11.so.6');lib.XOpenDisplay.argtypes=[ctypes.c_char_p];lib.XOpenDisplay.restype=ctypes.c_void_p
    lib.XInternAtom.argtypes=[ctypes.c_void_p,ctypes.c_char_p,ctypes.c_int];lib.XInternAtom.restype=ctypes.c_ulong
    lib.XGetSelectionOwner.argtypes=[ctypes.c_void_p,ctypes.c_ulong];lib.XGetSelectionOwner.restype=ctypes.c_ulong
    lib.XCloseDisplay.argtypes=[ctypes.c_void_p]
    display=lib.XOpenDisplay(env()['DISPLAY'].encode());assert display
    try:return lib.XGetSelectionOwner(display,lib.XInternAtom(display,b'CLIPBOARD',0))
    finally:lib.XCloseDisplay(display)
if op=='snapshot':
    pids=run(['pgrep','-u','zaza','-x','poolsync-agent']).stdout.decode().split()
    assert len(pids)==1,'expected one original desktop agent'
    pid=int(pids[0]);proc=Path('/proc')/str(pid)
    raw=user(['cat',str(proc/'environ')]);assert raw.returncode==0
    original_env=dict(item.split('=',1) for item in raw.stdout.decode().split('\0') if '=' in item)
    args=user(['cat',str(proc/'cmdline')]).stdout.decode().split('\0');args=[arg for arg in args if arg]
    cfg=Path('/home/zaza/.config/poolsync/agent.toml')
    marker=cfg.with_suffix('.away')
    output={'pid':pid,'args':args,'environment':original_env,'running_sha256':digest(proc/'exe'),'config_sha256':digest(cfg),'away':marker.read_text() if marker.exists() else None}
elif op=='prepare':
    root.mkdir(mode=0o700);home=root/'home';home.mkdir(mode=0o700);(root/'runtime').mkdir(mode=0o700)
    original=request['original'];environment=dict(original['environment'])
    environment.update(HOME=str(home),XDG_RUNTIME_DIR=str(root/'runtime'),XDG_CACHE_HOME=str(home/'.cache'),XDG_CONFIG_HOME=str(home/'.config'))
    display=request['display'];assert not Path('/tmp/.X11-unix/X'+str(display)).exists(),'test display already in use'
    environment.update(DISPLAY=':'+str(display),XAUTHORITY=str(root/'authority'),XDG_SESSION_TYPE='x11')
    (root/'authority').touch(mode=0o600)
    (root/'xorg.conf').write_text(request['xorg_config'])
    (root/'environment.json').write_text(json.dumps(environment));(root/'environment.json').chmod(0o600)
    (root/'agent.toml').write_text(request['config']);(root/'agent.toml').chmod(0o600)
    if 'layout' in request:
        (root/'agent.topology.json').write_text(json.dumps(request['layout']));(root/'agent.topology.json').chmod(0o600)
    import gi
    gi.require_version('GdkPixbuf','2.0')
    from gi.repository import GdkPixbuf
    output={}
    for name,w,h,color in [('first',193,127,0xb83268ff),('second',83,59,0x278ce4ff)]:
        p=GdkPixbuf.Pixbuf.new(GdkPixbuf.Colorspace.RGB,True,8,w,h);p.fill(color);p.savev(str(root/(name+'.png')),'png',[],[])
        output[name]={'kind':'image','sha256':hashlib.sha256(bytes([color>>24,(color>>16)&255,(color>>8)&255,color&255])*w*h).hexdigest(),'width':w,'height':h}
    subprocess.run(['chown','-R','zaza:zaza',str(root)],check=True)
elif op=='xserver':
    with (root/'xserver-console.log').open('ab') as stream:
        process=subprocess.Popen(['Xorg',env()['DISPLAY'],'-config',str(root/'xorg.conf'),'-noreset','-nolisten','tcp','-ac','-logfile',str(root/'xorg.log')],stdout=stream,stderr=stream,start_new_session=True)
    output=process.pid
elif op=='start':
    output=spawn([str(root/'candidate-agent'),'--config',str(root/'agent.toml')],'agent.log')
elif op=='receiver':
    output=spawn(['python3','/opt/poolsync-tests/paste-receiver/clipboard-receiver.py','--watch','--log',str(root/'pastes.json'),'--quit-after','600'],'receiver.log')
elif op=='stop':stop(request['pid']);output=True
elif op=='copy':
    raw=(root/(request['image']+'.png')).read_bytes() if 'image' in request else request['text'].encode()
    mime='image/png' if 'image' in request else 'UTF8_STRING'
    p=subprocess.Popen(['runuser','-m','-u','zaza','--','xclip','-selection','clipboard','-t',mime],env=env(),stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    p.communicate(raw,timeout=5);assert p.returncode==0;time.sleep(.15)
    # Selection owner reuse without TIMESTAMP can expose stale image-read caching.
    os.environ['XAUTHORITY']=env()['XAUTHORITY']
    owner=clipboard_owner();stamp=user(['xclip','-selection','clipboard','-t','TIMESTAMP','-o'],env=env()).stdout
    output={'owner':owner,'timestamp':stamp.decode().strip() if stamp.strip().isdigit() else None,'kind':'image' if 'image' in request else 'text'}
elif op=='pastes':
    try:output=json.loads((root/'pastes.json').read_text())
    except (FileNotFoundError,json.JSONDecodeError):output=[]
elif op=='text':
    code="import gi,json;gi.require_version('Gtk','3.0');from gi.repository import Gtk,Gdk;Gtk.init([]);print(json.dumps(Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text()))"
    r=user(['python3','-c',code],env=env());assert r.returncode==0;output=json.loads(r.stdout)
elif op=='away':
    r=user([str(root/'candidate-agent'),'--config',str(root/'agent.toml'),'--away',str(request['value']).lower()],env=env());assert r.returncode==0;output=True
elif op=='cache':
    directory=root/'home/.cache/poolsync/clipboard';output=sorted(p.name for p in directory.glob('*'))
elif op=='pointer':
    r=user(['xdotool','getmouselocation','--shell'],env=env());assert r.returncode==0
    values=dict(line.split('=',1) for line in r.stdout.decode().splitlines());output={'x':int(values['X']),'y':int(values['Y'])}
elif op=='move':
    r=user(['xdotool','mousemove',str(request['x']),str(request['y'])],env=env());assert r.returncode==0;output=True
elif op=='screen':
    r=user(['xdotool','getdisplaygeometry'],env=env());assert r.returncode==0;output=[int(n) for n in r.stdout.split()]
elif op=='status':
    try:output=json.loads((root/'agent.status.json').read_text())
    except (FileNotFoundError,json.JSONDecodeError):output={}
elif op=='browser':
    profile=root/'browser-profile';profile.mkdir(mode=0o700)
    (profile/'user.js').write_text('user_pref("browser.shell.checkDefaultBrowser",false);user_pref("browser.startup.homepage_override.mstone","ignore");user_pref("browser.aboutwelcome.enabled",false);user_pref("datareporting.policy.firstRunURL","");')
    subprocess.run(['chown','-R','zaza:zaza',str(profile)],check=True)
    server=spawn(['python3','/opt/poolsync-tests/paste-receiver/browser-paste-server.py','--html','/opt/poolsync-tests/paste-receiver/clipboard-paste.html','--log',str(root/'browser-pastes.json')],'browser-server.log')
    time.sleep(.3)
    browser=spawn(['firefox','--no-remote','--profile',str(profile),'--new-window','http://127.0.0.1:19580/'],'browser.log')
    output={'server':server,'browser':browser}
elif op=='browser_paste':
    r=user(['xdotool','search','--name','PoolSync clipboard paste test'],env=env());assert r.returncode==0
    window=r.stdout.decode().splitlines()[-1]
    r=user(['xdotool','windowfocus','--sync',window],env=env());assert r.returncode==0
    r=user(['xdotool','key','ctrl+v'],env=env());assert r.returncode==0;output=True
elif op=='browser_records':
    try:output=json.loads((root/'browser-pastes.json').read_text())
    except (FileNotFoundError,json.JSONDecodeError):output=[]
elif op=='key':
    r=user(['xdotool',request.get('action','key'),request['key']],env=env());assert r.returncode==0;output=True
elif op=='motion':
    r=user(['xdotool','mousemove_relative','--',str(request['dx']),str(request['dy'])],env=env());assert r.returncode==0;output=True
elif op=='key_window':
    code="import gi,json,sys;gi.require_version('Gtk','3.0');from gi.repository import Gtk,GLib;w=Gtk.Window(title='PoolSync remote keyboard receiver');w.connect('key-press-event',lambda _,event: (open(sys.argv[1],'w').write(json.dumps({'keyval':event.keyval})),False)[1]);w.show_all();GLib.timeout_add_seconds(600,lambda: (Gtk.main_quit(),False)[1]);Gtk.main()"
    marker=root/'remote-key.json';marker.unlink(missing_ok=True)
    output=spawn(['python3','-c',code,str(marker)],'remote-key.log')
    for _ in range(50):
        r=user(['xdotool','search','--name','^PoolSync remote keyboard receiver$'],env=env())
        if r.returncode==0:break
        time.sleep(.1)
    assert r.returncode==0
    r=user(['xdotool','windowfocus','--sync',r.stdout.decode().splitlines()[-1]],env=env());assert r.returncode==0
elif op=='received_key':
    try:output=json.loads((root/'remote-key.json').read_text())
    except (FileNotFoundError,json.JSONDecodeError):output={}
elif op=='key_state':
    code="import ctypes,os,sys,json;l=ctypes.CDLL('libX11.so.6');l.XOpenDisplay.argtypes=[ctypes.c_char_p];l.XOpenDisplay.restype=ctypes.c_void_p;l.XStringToKeysym.argtypes=[ctypes.c_char_p];l.XStringToKeysym.restype=ctypes.c_ulong;l.XKeysymToKeycode.argtypes=[ctypes.c_void_p,ctypes.c_ulong];l.XKeysymToKeycode.restype=ctypes.c_ubyte;l.XQueryKeymap.argtypes=[ctypes.c_void_p,ctypes.c_void_p];l.XCloseDisplay.argtypes=[ctypes.c_void_p];d=l.XOpenDisplay(os.environ['DISPLAY'].encode());assert d;k=l.XKeysymToKeycode(d,l.XStringToKeysym(sys.argv[1].encode()));m=ctypes.create_string_buffer(32);l.XQueryKeymap(d,m);l.XCloseDisplay(d);print(json.dumps(bool(m.raw[k//8]&(1<<(k%8)))))"
    r=user(['python3','-c',code,request['key']],env=env());assert r.returncode==0;output=json.loads(r.stdout)
elif op=='local_key':
    code="import gi,json,sys;gi.require_version('Gtk','3.0');from gi.repository import Gtk,GLib;w=Gtk.Window(title='PoolSync isolated local input proof');w.connect('key-press-event',lambda _,event: (open(sys.argv[1],'w').write(json.dumps({'keyval':event.keyval})),False)[1]);w.show_all();GLib.timeout_add_seconds(8,lambda: (Gtk.main_quit(),False)[1]);Gtk.main()"
    marker=root/'key.json';marker.unlink(missing_ok=True)
    pid=spawn(['python3','-c',code,str(marker)],'key.log')
    try:
        time.sleep(.6)
        r=user(['xdotool','search','--name','^PoolSync isolated local input proof$'],env=env());assert r.returncode==0
        window=r.stdout.decode().splitlines()[-1]
        r=user(['xdotool','windowfocus','--sync',window],env=env());assert r.returncode==0
        r=user(['xdotool','key','z'],env=env());assert r.returncode==0
        for _ in range(20):
            if marker.exists():break
            time.sleep(.1)
        output=marker.exists() and json.loads(marker.read_text())['keyval']==ord('z')
    finally:stop(pid)
elif op=='health':
    log=(root/'agent.log').read_text(errors='replace');listener=False
    try:
        with socket.create_connection(('127.0.0.1',request['port']),timeout=1):listener=True
    except OSError:pass
    try:
        with socket.create_connection(('127.0.0.1',1),timeout=.5):hub=True
    except OSError:hub=False
    pids=user(['pgrep','-f','^'+str(root)+'/candidate-agent ']).stdout.decode().split();isolated=False
    if len(pids)==1:
        raw=user(['cat','/proc/'+pids[0]+'/environ']).stdout.decode()
        actual=dict(item.split('=',1) for item in raw.split('\0') if '=' in item)
        isolated=actual.get('HOME')==str(root/'home') and actual.get('DISPLAY')==env()['DISPLAY'] and actual.get('XDG_RUNTIME_DIR')==str(root/'runtime')
    output={'listener':listener,'hub_reachable':hub,'hub_connected_logged':'connected to hub' in log,'isolated_environment':isolated,'mesh_links':log.count('peer mesh connecté'),'panic':'thread ' in log and 'panicked' in log}
elif op=='restore':
    original=request['original'];cfg=Path('/home/zaza/.config/poolsync/agent.toml')
    assert digest(cfg)==original['config_sha256'],'original configuration changed'
    marker=cfg.with_suffix('.away');assert (marker.read_text() if marker.exists() else None)==original['away'],'original participation changed'
    existing=run(['pgrep','-u','zaza','-x','poolsync-agent']).stdout.decode().split()
    if not existing:
        subprocess.Popen(['runuser','-m','-u','zaza','--',*original['args']],env=original['environment'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
        time.sleep(1)
    pids=run(['pgrep','-u','zaza','-x','poolsync-agent']).stdout.decode().split();assert len(pids)==1
    assert digest('/proc/'+pids[0]+'/exe')==original['running_sha256'],'restored agent differs'
    output={'config_preserved':True,'original_agent_restored':True,'original_pid_preserved':int(pids[0])==original['pid'],'away_preserved':True}
else:raise RuntimeError('unknown operation')
print(json.dumps(output))
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', required=True, type=Path)
    parser.add_argument('--expected-sha256', required=True)
    parser.add_argument('--containers', nargs=3, default=['neko-desk-a', 'neko-desk-b', 'neko-desk-c'])
    parser.add_argument('--addresses', nargs=3, required=True)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--display', type=int, default=110, help='Unused private Xorg display in each container')
    parser.add_argument('--hubless', action='store_true', help='Qualify direct peer control, not the legacy clipboard-only mesh')
    parser.add_argument('--clipboard-rounds', type=int, default=0, help='Additional alternating native image/text rounds')
    parser.add_argument('--native-browser', action='store_true', help='Use an isolated Firefox profile and native Ctrl+V on B')
    parser.add_argument('--kvm-only', action='store_true', help='Run the native control and recovery scenarios without clipboard scenarios')
    args = parser.parse_args()
    assert not args.kvm_only or args.hubless, '--kvm-only requires --hubless'
    assert os.geteuid() == 0, 'Run on the disposable Podman host as root'
    actual = hashlib.file_digest(args.candidate.open('rb'), 'sha256').hexdigest()
    assert actual == args.expected_sha256, 'Unexpected candidate binary'
    run_id = uuid.uuid4().hex[:12]
    root = '/tmp/poolsync-no-hub-' + run_id
    artifacts = args.output.parent / ('private-' + run_id)
    artifacts.mkdir(mode=0o700, parents=True)
    nodes = ['nohub-a', 'nohub-b', 'nohub-c']
    tokens = {node: secrets.token_hex(24) for node in nodes}
    e2e_key = base64.b64encode(secrets.token_bytes(32)).decode()
    port = 19676
    snapshots, agents, receivers, images, xservers = {}, {}, {}, {}, {}
    browser_workers = {}
    checked, checks, restoration = set(), [], {}
    latencies, copies = [], []
    result = {'candidate_sha256': actual, 'production_hub_used': False, 'hub_available_during_test': False,
              'serverless_kvm': None, 'input_source': 'synthetic X11 events', 'restart_scope': 'agents',
              'isolated_xorg_display': args.display,
              'receiver_mode': 'native GTK paste handler sampled once per second',
              'checks': checks, 'paste_convergence_seconds': latencies, 'native_copy_owners': copies}

    def op(index, operation, **extra):
        request = {'root': root, 'op': operation, **extra}
        process = subprocess.run(['podman', 'exec', '-i', args.containers[index], 'python3', '-c', HELPER],
                                 input=json.dumps(request), capture_output=True, text=True, timeout=18)
        if process.returncode:
            # Keep diagnostic output private: snapshots contain inherited environment values.
            (artifacts / ('error-' + str(index) + '-' + operation + '.log')).write_text(process.stderr)
            raise RuntimeError(f'{args.containers[index]}: {operation} failed; see private diagnostics')
        response=json.loads(process.stdout)
        if operation == 'copy':
            copies.append({'container':args.containers[index],**response})
        return response

    def check(name, condition=True):
        entry = {'name': name, 'passed': bool(condition)}
        checks.append(entry)
        print(json.dumps(entry), flush=True)
        assert condition, name

    def attempt(name, action):
        try:
            action()
            check(name)
            return True
        except AssertionError as error:
            entry={'name':name,'passed':False,'error':str(error)}
            checks.append(entry)
            print(json.dumps(entry),flush=True)
            return False

    def expect(indices, expected, since, timeout=25):
        deadline = time.monotonic() + timeout
        missing = set(indices)
        while missing and time.monotonic() < deadline:
            for index in list(missing):
                if any(entry['at'] >= since and all(entry.get(k) == v for k, v in expected.items())
                       for entry in op(index, 'pastes')):
                    missing.remove(index)
            if missing:
                time.sleep(.2)
        assert not missing, 'Native paste did not converge on ' + ', '.join(args.containers[i] for i in missing)
        latencies.append(round(time.time() - since, 3))

    def copy_text(index, label, targets=(0, 1, 2)):
        text = label + ' ' + uuid.uuid4().hex
        expected = {'kind': 'text', 'sha256': hashlib.sha256(text.encode()).hexdigest()}
        started = time.time()
        op(index, 'copy', text=text)
        expect(targets, expected, started)
        return text

    def restart(index):
        links=op(index,'health',port=port)['mesh_links']
        op(index, 'stop', pid=agents.pop(index))
        agents[index] = op(index, 'start')
        deadline=time.monotonic()+20
        while time.monotonic()<deadline:
            status=op(index,'health',port=port)
            if status['isolated_environment'] and status['listener'] and status['mesh_links']>links:return
            time.sleep(.25)
        raise AssertionError('Restarted agent did not establish a new direct peer link')

    def wait_lease(owner, focus, indices=(0,1), timeout=10):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            leases=[op(i,'status').get('lease') for i in indices]
            if all(l and l['owner']==nodes[owner] and l['focus']==nodes[focus] for l in leases):return
            time.sleep(.2)
        raise AssertionError('Peer input leases did not converge')

    try:
        for index in range(3):
            snapshots[index] = op(index, 'snapshot')
        (artifacts / 'original-agents.json').write_text(json.dumps(snapshots))
        (artifacts / 'original-agents.json').chmod(0o600)
        for index, node in enumerate(nodes):
            peers = [j for j in range(3) if abs(j - index) == 1]
            lines = [f'node={json.dumps(node)}', 'hub_url="ws://127.0.0.1:1/ws"', 'token="isolated-unused-hub-token"',
                     'hubless='+str(args.hubless).lower(),
                     f'node_token={json.dumps(tokens[node])}', f'e2e_key={json.dumps(e2e_key)}',
                     'mode="clipboard_only"' if index == 2 else 'mode="full"',
                     'kvm_enabled=' + str(index != 2).lower(), 'kvm_capture=' + str(index != 2).lower(),
                     f'peer_listen_port={port}', 'peer_direct_clipboard=true', 'hub_clipboard=false',
                     'clipboard_poll_ms=80', 'pause_clipboard_when_rdp=false', 'input_poll_ms=8',
                     '[screen]', 'width=1600', 'height=900', '[peer_tokens]']
            lines += [f'{nodes[j]}={json.dumps(tokens[nodes[j]])}' for j in range(3) if j != index]
            for j in peers:
                lines += ['[[neighbors]]', f'node={json.dumps(nodes[j])}',
                          'direction="right"' if j > index else 'direction="left"',
                          f'peer_url="ws://{args.addresses[j]}:{port}/ws"']
            layout={'revision':1,'origin':'nohub-a','topology':{'nodes':{name:{'x':j*1600,'y':0,'width':1600,'height':900,'kvm_enabled':j!=2} for j,name in enumerate(nodes)}}}
            images[index] = op(index, 'prepare', original=snapshots[index], config='\n'.join(lines) + '\n',display=args.display,xorg_config=XORG_CONFIG,layout=layout)
            subprocess.run(['podman', 'cp', str(args.candidate), args.containers[index] + ':' + root + '/candidate-agent'], check=True, capture_output=True)
            subprocess.run(['podman', 'exec', args.containers[index], 'chmod', '755', root + '/candidate-agent'], check=True, capture_output=True)
        for index in range(3):
            checked.add(index)
            xservers[index]=op(index,'xserver')
        time.sleep(1.5)
        for index in (2, 0, 1):
            agents[index] = op(index, 'start')
            receivers[index] = op(index, 'receiver')
        time.sleep(4)
        health = [op(i, 'health', port=port) for i in range(3)]
        check('Cold start: isolated agents run with hub unreachable', all(h['listener'] and h['isolated_environment'] and not h['hub_reachable'] and not h['hub_connected_logged'] for h in health))
        if not args.kvm_only:
            copy_text(0, 'Cold-start A to B to C')
            check('Encrypted native text crosses the A-B-C chain without hub')
            copy_text(2, 'Clipboard-only C to B to A')
            check('Clipboard-only node sends native text in the reverse direction')
            for image in ('first', 'second'):
                started = time.time();op(0, 'copy', image=image)
                name='GTK receivers paste ' + image + ' image with identical RGBA pixels'
                try:
                    expect((0, 1, 2), images[0][image], started)
                    check(name)
                except AssertionError as error:
                    entry={'name':name,'passed':False,'error':str(error)};checks.append(entry)
                    print(json.dumps(entry),flush=True)
                time.sleep(1)
            text_transition = attempt('Native text replaces an image on every desktop',
                                      lambda: copy_text(1, 'Text after two screenshots'))
            if not text_transition:
                # Keep the rapid-transition failure; establish a settled baseline
                # so absence and restart scenarios still get independent coverage.
                time.sleep(6)
                copy_text(1,'Settled text baseline after failed rapid transition')
            started = time.time();op(2, 'copy', image='first');expect((0, 1, 2), images[2]['first'], started)
            check('Image after text travels from clipboard-only C to A')

            if args.native_browser:
                browser_workers=op(1,'browser');time.sleep(8)
                for image in ('second','first'):
                    started=time.time();op(0,'copy',image=image);expect((0,1,2),images[0][image],started)
                    op(1,'browser_paste');deadline=time.monotonic()+8
                    while time.monotonic()<deadline:
                        if any(e['at']>=started and e.get('sha256')==images[0][image]['sha256'] for e in op(1,'browser_records')):break
                        time.sleep(.2)
                    else:raise AssertionError('Native Firefox did not paste the '+image+' image')
                    check('Native Firefox Ctrl+V pastes the '+image+' image with matching RGBA pixels')
                text=copy_text(0,'Native browser text after screenshots');started=time.time();op(1,'browser_paste');deadline=time.monotonic()+8
                while time.monotonic()<deadline:
                    if any(e['at']>=started and e.get('sha256')==hashlib.sha256(text.encode()).hexdigest() for e in op(1,'browser_records')):break
                    time.sleep(.2)
                else:raise AssertionError('Native Firefox did not paste the fresh text')
                check('Native Firefox Ctrl+V pastes text after images')
                started=time.time();op(0,'copy',image='first');expect((0,1,2),images[0]['first'],started);op(1,'browser_paste');deadline=time.monotonic()+8
                while time.monotonic()<deadline:
                    if any(e['at']>=started and e.get('sha256')==images[0]['first']['sha256'] for e in op(1,'browser_records')):break
                    time.sleep(.2)
                else:raise AssertionError('Native Firefox did not paste an image after text')
                check('Native Firefox Ctrl+V pastes an image after text')
                result['native_browser_paste']=True

        if args.hubless:
            check('All three peers discover each other through the chain',all(len(op(i,'status').get('peers',{}))==3 for i in range(3)))
            receivers[3]=op(1,'key_window')
            op(1,'move',x=300,y=250);width,height=op(0,'screen')
            op(0,'move',x=width//2,y=height//2);op(0,'key',key='ctrl+alt+shift+m');time.sleep(.5)
            op(0,'move',x=width-1,y=height//2)
            wait_lease(0,1)
            result['kvm_entry_pointer']=op(1,'pointer')
            check('A crosses to B with peer-controlled ownership and no hub',result['kvm_entry_pointer']['x']<100)
            before=op(1,'pointer');op(0,'motion',dx=70,dy=35);time.sleep(.4)
            check('Grabbed A motion moves the actual B pointer',op(1,'pointer')!=before)
            op(0,'key',key='z');time.sleep(.4)
            check('A keyboard reaches the real B GTK window',op(1,'received_key').get('keyval')==ord('z'))
            op(0,'key',key='ctrl+alt+shift+m');wait_lease(0,0)
            check('Emergency shortcut returns captured input to A',op(0,'local_key'))
            check('Emergency return releases remote modifiers',all(not op(1,'key_state',key=k) for k in ('Shift_L','Control_L','Alt_L')))
            op(0,'move',x=width//2,y=height//2);time.sleep(.8);op(0,'move',x=width-1,y=height//2);wait_lease(0,1)
            op(0,'key',action='keydown',key='Shift_L');time.sleep(.4)
            check('A held modifier is injected on B',op(1,'key_state',key='Shift_L'))
            op(0,'stop',pid=agents.pop(0));time.sleep(5)
            check('Controller disappearance releases the held modifier on B',not op(1,'key_state',key='Shift_L'))
            check('B native keyboard is usable after the controller disappears',op(1,'local_key'))
            agents[0]=op(0,'start');time.sleep(4)
            op(1,'move',x=800,y=450);op(1,'key',key='ctrl+alt+shift+m');time.sleep(.5)
            op(1,'move',x=0,y=450);wait_lease(1,0)
            check('B takes control and crosses to A after A restarts')
            op(0,'away',value=True);time.sleep(5)
            check('A temporary departure returns B input locally',op(1,'status').get('lease') is None or op(1,'status')['lease']['focus']==nodes[1])
            check('B local GTK keyboard works after its remote target leaves',op(1,'local_key'))
            op(0,'away',value=False);time.sleep(2)
            result['serverless_kvm']=True
            result['kvm_observation']='Direct peer edge crossing, native GTK key reception, held-key release and target/controller recovery. Input is synthetic X11, not physical acceptance.'
        else:
            op(1, 'move', x=300, y=250);before=op(1, 'pointer');width,height=op(0, 'screen')
            op(0, 'move', x=width//2, y=height//2);time.sleep(.4)
            op(0, 'move', x=width-1, y=height//2);time.sleep(1)
            check('Hubless edge crossing leaves the other desktop pointer unchanged', op(1, 'pointer') == before)
            check('Local GTK keyboard input remains usable with hub absent', op(0, 'local_key'))
            result['serverless_kvm'] = False
            result['kvm_observation'] = 'No cross-desktop input; local input remains available. KVM is hub-dependent.'

        if not args.kvm_only:
            if args.clipboard_rounds:copy_text(0,'Clipboard baseline before endurance rounds')
            for round_index in range(args.clipboard_rounds):
                i=round_index%3;image='first' if round_index%2==0 else 'second'
                started=time.time();op(i,'copy',image=image);expect((0,1,2),images[i][image],started)
                copy_text((i+1)%3,'Repeated image/text round '+str(round_index))
            if args.clipboard_rounds:check('Repeated native image/text transitions',True)

            copy_text(0, 'Before temporary absence')
            # Let already accepted pre-absence verification finish before measuring new copies.
            time.sleep(6)
            op(2, 'away', value=True);time.sleep(1.5);cache=op(2, 'cache')
            office = copy_text(0, 'Office copy while C absent', targets=(0, 1));time.sleep(1)
            # Leaving deliberately releases GTK ownership; the old selection may be empty.
            check('Absent C neither receives nor retains office copies', op(2, 'text') != office and op(2, 'cache') == cache)
            private = copy_text(2, 'Private copy while C absent', targets=(2,));time.sleep(1)
            check('Absent C does not send its private copy', op(0, 'text') != private and op(1, 'text') != private)
            restart(2);time.sleep(1)
            copy_text(0, 'Office copy after absent C restart', targets=(0, 1));time.sleep(1)
            check('Absence survives agent restart without hub', op(2, 'text') == private and op(2, 'cache') == cache)
            op(2, 'away', value=False);time.sleep(2)
            check('Rejoin does not replay the private clipboard', op(0, 'text') != private and op(1, 'text') != private)
            attempt('Fresh native copies resume after rejoin without hub',
                    lambda:copy_text(0, 'Fresh copy after rejoin'))

            op(1, 'stop', pid=agents.pop(1));time.sleep(1)
            isolated = copy_text(0, 'Copy while relay B offline', targets=(0,));time.sleep(2)
            check('A local clipboard remains usable after the relay agent disappears', op(0, 'text') == isolated and op(2, 'text') != isolated)
            links=op(1,'health',port=port)['mesh_links']
            agents[1]=op(1, 'start')
            deadline=time.monotonic()+20
            while time.monotonic()<deadline:
                status=op(1,'health',port=port)
                if status['isolated_environment'] and status['mesh_links']>=links+2:break
                time.sleep(.25)
            attempt('Direct links recover after relay restart without hub',
                    lambda:copy_text(2, 'Fresh copy after relay restart'))
            restart(0)
            attempt('Reverse-order agent restart restores native mesh delivery',
                    lambda:copy_text(2, 'Fresh copy after A restart'))
        health=[op(i, 'health', port=port) for i in range(3)]
        check('Hub stayed unreachable throughout; agents report no panic', all(not h['hub_reachable'] and not h['hub_connected_logged'] and not h['panic'] for h in health))
        result['functional_checks_passed'] = all(entry['passed'] for entry in checks)
    except Exception as error:
        result['functional_checks_passed'] = False
        result['error'] = str(error)
        print(json.dumps({'test_error': str(error)}), flush=True)
    finally:
        for pid in browser_workers.values():
            try:op(1,'stop',pid=pid)
            except Exception as error:restoration['browser-stop-error']=str(error)
        # The remote keyboard receiver lives on B, independently of its key.
        if 3 in receivers:
            try:op(1,'stop',pid=receivers.pop(3))
            except Exception as error:restoration['key-window-stop-error']=str(error)
        for processes in (receivers, agents, xservers):
            for index, pid in list(processes.items()):
                try:op(index, 'stop', pid=pid)
                except Exception as error:restoration[str(index) + '-stop-error']=str(error)
        for index in sorted(checked):
            try:restoration[args.containers[index]]=op(index, 'restore', original=snapshots[index])
            except Exception as error:restoration[args.containers[index]]={'error':str(error)}
        result['restoration']=restoration
        result['restoration_passed']=len(restoration)==len(checked) and all(r.get('original_agent_restored') and r.get('original_pid_preserved') for r in restoration.values() if isinstance(r,dict))
        result['checks_passed']=sum(check['passed'] for check in checks)
        result['test_root']=root
        args.output.write_text(json.dumps(result, indent=2) + '\n');args.output.chmod(0o600)
        print(json.dumps({'summary':result}), flush=True)
    raise SystemExit(0 if result.get('functional_checks_passed') and result['restoration_passed'] else 1)


if __name__ == '__main__':
    main()
