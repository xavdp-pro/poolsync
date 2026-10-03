#!/usr/bin/env python3
"""Qualify an isolated no-hub mesh on three disposable Podman X11 desktops.

Run inside the test host, with root access to Podman. Existing desktop agents
remain running and are verified in finally; their configuration is never
rewritten. Candidate Xorg display, HOME, credentials, locks and history are isolated.
The configured hub is loopback port 1 in each desktop, checked unreachable.
Complete container restarts require --fresh-desktops --reboot-desktops.
Synthetic X11 input does not qualify physical input devices.
"""

import argparse
from concurrent.futures import ThreadPoolExecutor
import base64
import hashlib
import json
import os
from pathlib import Path
import secrets
import subprocess
import time
import uuid


HARNESS_SHA256 = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()


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
def ui_window():
    # IPC acknowledges presentation intent before GTK maps the window.
    deadline=time.monotonic()+4
    while time.monotonic()<deadline:
        r=user(['xdotool','search','--onlyvisible','--name','^PoolSync — nohub-a$'],env=env())
        if r.returncode==0 and r.stdout.strip():return r.stdout.decode().splitlines()[-1]
        time.sleep(.05)
    raise AssertionError('Native configuration window did not become visible')
def spawn(args,log):
    # Keep a parent waiting on the worker so container PID 1 cannot leave an
    # unreaped agent PID that its existing single-instance guard sees as alive.
    pid_file=root/(log+'.pid.'+str(os.getpid()))
    guardian="import os,subprocess,sys;from pathlib import Path;p=subprocess.Popen(sys.argv[2:],user='zaza',group='zaza',extra_groups=[],start_new_session=True);Path(sys.argv[1]).write_text(str(p.pid));p.wait()"
    environment=env()
    if log=='agent.log' and (root/'bounded-allocator').exists():
        environment.update(MALLOC_ARENA_MAX='2',MALLOC_MMAP_THRESHOLD_='131072')
    if log=='browser.log' and (root/'browser-engine').read_text()=='firefox':
        # Private synthetic-data trace distinguishes a browser clipboard
        # request timeout/cache rejection from an image-decoder failure.
        environment['MOZ_LOG']='WidgetClipboard:5'
        environment.pop('MOZ_LOG_FILE',None)
    with (root/log).open('ab') as stream:
        process=subprocess.Popen(['python3','-c',guardian,str(pid_file),*args],env=environment,stdout=stream,stderr=stream,start_new_session=True)
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
    fresh=request.get('fresh',False)
    assert len(pids)==(0 if fresh else 1),'unexpected existing desktop agent count'
    pid=None if fresh else int(pids[0])
    if fresh:
        original_env={'PATH':'/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin','USER':'zaza','LOGNAME':'zaza','HOME':'/home/zaza'}
        args=[];running_sha=None
    else:
        proc=Path('/proc')/str(pid)
        raw=user(['cat',str(proc/'environ')]);assert raw.returncode==0
        original_env=dict(item.split('=',1) for item in raw.stdout.decode().split('\0') if '=' in item)
        args=user(['cat',str(proc/'cmdline')]).stdout.decode().split('\0');args=[arg for arg in args if arg]
        running_sha=digest(proc/'exe')
    cfg=Path('/home/zaza/.config/poolsync/agent.toml')
    marker=cfg.with_suffix('.away')
    output={'pid':pid,'fresh':fresh,'args':args,'environment':original_env,'running_sha256':running_sha,'config_sha256':digest(cfg) if cfg.exists() else None,'away':marker.read_text() if marker.exists() else None}
elif op=='prepare':
    root.mkdir(mode=0o700);(root/'slow-image-owner.py').write_text(request['slow_fixture']);home=root/'home';home.mkdir(mode=0o700);(root/'runtime').mkdir(mode=0o700)
    (root/'lossless-image-owner.py').write_text(request['lossless_fixture'])
    original=request['original'];environment=dict(original['environment'])
    environment.update(HOME=str(home),XDG_RUNTIME_DIR=str(root/'runtime'),XDG_CACHE_HOME=str(home/'.cache'),XDG_CONFIG_HOME=str(home/'.config'))
    environment['DBUS_SESSION_BUS_ADDRESS']='unix:path='+str(root/'runtime/bus')
    environment.pop('DBUS_STARTER_ADDRESS',None);environment.pop('DBUS_STARTER_BUS_TYPE',None)
    environment['GTK_USE_PORTAL']='0'
    environment['RUST_LOG']='poolsync_agent=info,poolsync_agent::clipboard=debug'
    if request.get('trace_transport'):environment['RUST_LOG']+=',poolsync_agent::peer_mesh=debug'
    if request.get('tls'):
        for name,contents in request['tls'].items():
            assert name in ('peer-ca.pem','peer.pem','peer.key')
            path=root/name;path.write_text(contents);path.chmod(0o600)
        (root/'empty-cert-directory').mkdir(mode=0o700)
        environment.update(SSL_CERT_FILE=str(root/'peer-ca.pem'),SSL_CERT_DIR=str(root/'empty-cert-directory'))
    display=request['display'];assert not Path('/tmp/.X11-unix/X'+str(display)).exists(),'test display already in use'
    environment.update(DISPLAY=':'+str(display),XAUTHORITY=str(root/'authority'),XDG_SESSION_TYPE='x11')
    (root/'authority').touch(mode=0o600)
    (root/'xorg.conf').write_text(request['xorg_config'])
    (root/'environment.json').write_text(json.dumps(environment));(root/'environment.json').chmod(0o600)
    (root/'agent.toml').write_text(request['config']);(root/'agent.toml').chmod(0o600)
    if request.get('show_window'):(root/'show-window').touch()
    if request.get('native_owner')=='gtk':(root/'gtk-native-owner').touch()
    if request.get('browser_x11_trace'):(root/'browser-x11-trace').touch()
    if request.get('browser_syscall_trace'):(root/'browser-syscall-trace').touch()
    if request.get('bounded_allocator'):(root/'bounded-allocator').touch()
    if request.get('bandwidth_proxy'):(root/'peer-bandwidth-proxy.py').write_text(request['bandwidth_proxy'])
    if request.get('browser_fixtures'):
        (root/'browser-engine').write_text(request.get('browser_engine','firefox'))
        browser_port=int(request.get('browser_port',19580));assert 1024<=browser_port<=65535
        (root/'browser-port').write_text(str(browser_port))
        for name,contents in request['browser_fixtures'].items():
            assert name in ('browser-paste-server.py','clipboard-paste.html')
            (root/name).write_text(contents)
    if 'layout' in request:
        (root/'agent.topology.json').write_text(json.dumps(request['layout']));(root/'agent.topology.json').chmod(0o600)
    import gi
    gi.require_version('GdkPixbuf','2.0')
    from gi.repository import GdkPixbuf
    output={}
    for name,w,h,color in [('first',193,127,0xb83268ff),('second',83,59,0x278ce4ff)]:
        p=GdkPixbuf.Pixbuf.new(GdkPixbuf.Colorspace.RGB,True,8,w,h);p.fill(color);p.savev(str(root/(name+'.png')),'png',[],[])
        output[name]={'kind':'image','sha256':hashlib.sha256(bytes([color>>24,(color>>16)&255,(color>>8)&255,color&255])*w*h).hexdigest(),'width':w,'height':h}
    if request.get('large_images'):
        from gi.repository import GLib
        for name in ('large-first','large-second'):
            w,h=1920,1080;pixels=bytearray(os.urandom(w*h*4));pixels[3::4]=b'\xff'*(w*h)
            p=GdkPixbuf.Pixbuf.new_from_bytes(GLib.Bytes.new(bytes(pixels)),GdkPixbuf.Colorspace.RGB,True,8,w,h,w*4)
            path=root/(name+'.png');p.savev(str(path),'png',[],[])
            output[name]={'kind':'image','sha256':hashlib.sha256(pixels).hexdigest(),'width':w,'height':h}
    subprocess.run(['chown','-R','zaza:zaza',str(root)],check=True)
elif op=='xserver':
    with (root/'xserver-console.log').open('ab') as stream:
        process=subprocess.Popen(['Xorg',env()['DISPLAY'],'-config',str(root/'xorg.conf'),'-noreset','-nolisten','tcp','-ac','-logfile',str(root/'xorg.log')],stdout=stream,stderr=stream,start_new_session=True)
    output=process.pid
elif op=='start':
    output=spawn([str(root/'candidate-agent'),'--config',str(root/'agent.toml'),*(['--show-window'] if (root/'show-window').exists() else [])],'agent.log')
elif op=='dbus':
    output=spawn(['dbus-daemon','--session','--nofork','--address='+env()['DBUS_SESSION_BUS_ADDRESS']],'dbus.log')
elif op=='cold_reset':
    assert request.get('fresh'), 'cold reset is restricted to disposable fresh desktops'
    assert not run(['pgrep','-x','Xorg']).stdout, 'an X server still runs'
    display=int(env()['DISPLAY'].lstrip(':').split('.')[0])
    for path in [root/'runtime/poolsync-agent.pid',root/'runtime/bus',Path('/tmp/.X'+str(display)+'-lock'),Path('/tmp/.X11-unix/X'+str(display))]:
        path.unlink(missing_ok=True)
    output=True
elif op=='receiver':
    output=spawn(['python3','/opt/poolsync-tests/paste-receiver/clipboard-receiver.py','--watch','--log',str(root/'pastes.json'),'--quit-after','7200'],'receiver.log')
elif op=='bandwidth_proxy':
    output=spawn(['python3',str(root/'peer-bandwidth-proxy.py'),'--listen-port',str(request['listen_port']),
                  '--backend-port',str(request['backend_port']),'--rate-mbit',str(request['rate_mbit'])],'bandwidth-proxy.log')
    deadline=time.monotonic()+3
    while True:
        try:
            with socket.create_connection(('127.0.0.1',request['listen_port']),timeout=.2):break
        except OSError:
            assert time.monotonic()<deadline, 'private bandwidth proxy did not listen'
            time.sleep(.05)
elif op=='process_alive':output=Path('/proc/'+str(request['pid'])).exists()
elif op=='stop':stop(request['pid']);output=True
elif op=='crash':
    try:os.killpg(request['pid'],signal.SIGKILL)
    except ProcessLookupError:pass
    output=True
elif op=='copy':
    raw=(root/(request['image']+'.png')).read_bytes() if 'image' in request else request['text'].encode()
    mime='image/png' if 'image' in request else 'UTF8_STRING'
    if request.get('bmp'):
        import gi;gi.require_version('GdkPixbuf','2.0');from gi.repository import GdkPixbuf
        ok,raw=GdkPixbuf.Pixbuf.new_from_file(str(root/(request['image']+'.png'))).save_to_bufferv('bmp',[],[]);assert ok
        mime='image/bmp'
    # Keep the native owner in foreground under a waiting parent. A daemon
    # fork must not disappear silently with the short-lived Podman exec helper.
    owner_before=clipboard_owner()
    previous=root/'native-copier-pid'
    previous_pid=int(previous.read_text()) if previous.exists() else None
    source=root/('native-copy-'+str(os.getpid()))
    source.write_bytes(raw);source.chmod(0o644)
    if (root/'gtk-native-owner').exists() and not request.get('bmp'):
        code="import gi,sys;gi.require_version('Gtk','3.0');from gi.repository import Gtk,Gdk,GdkPixbuf;Gtk.init([]);c=Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD);c.set_image(GdkPixbuf.Pixbuf.new_from_file(sys.argv[1])) if sys.argv[2]=='image/png' else c.set_text(open(sys.argv[1]).read(),-1);Gtk.main()"
        pid=spawn(['python3','-c',code,str(source),mime],'native-copy.log')
    else:
        pid=spawn(['xclip','-selection','clipboard','-t',mime,'-quiet',str(source)],'native-copy.log')
    (root/'native-copier-pid').write_text(str(pid))
    deadline=time.monotonic()+5
    while (clipboard_owner()==0 or clipboard_owner()==owner_before) and time.monotonic()<deadline:time.sleep(.05)
    assert clipboard_owner() and clipboard_owner()!=owner_before, 'copying application did not own a fresh selection'
    if previous_pid and (root/'gtk-native-owner').exists():stop(previous_pid)
    # Selection owner reuse without TIMESTAMP can expose stale image-read caching.
    os.environ['XAUTHORITY']=env()['XAUTHORITY']
    owner=clipboard_owner();stamp=b'' # xclip does not advertise TIMESTAMP; do not abandon a PNG INCR transfer
    output={'owner':owner,'copier_pid':pid,'timestamp':stamp.decode().strip() if stamp.strip().isdigit() else None,'kind':'image' if 'image' in request else 'text'}
elif op=='screenshot':
    destination=root/('native-screen-'+str(os.getpid())+'.png')
    command=['flameshot','full','-c','-p',str(destination)] if request.get('app')=='flameshot' else ['xfce4-screenshooter','-f','-c','-s',str(destination)]
    r=user(command,env=env());assert r.returncode==0,r.stderr.decode()
    import gi;gi.require_version('GdkPixbuf','2.0');from gi.repository import GdkPixbuf
    p=GdkPixbuf.Pixbuf.new_from_file(str(destination));p=p if p.get_has_alpha() else p.add_alpha(False,0,0,0)
    width,height=p.get_width(),p.get_height();raw=p.get_pixels();stride=p.get_rowstride();rgba=b''.join(raw[y*stride:y*stride+width*4] for y in range(height))
    output={'kind':'image','sha256':hashlib.sha256(rgba).hexdigest(),'width':width,'height':height}
elif op=='slow_image':
    marker=root/'slow-image-requested';marker.unlink(missing_ok=True)
    output=spawn(['python3',str(root/'slow-image-owner.py'),'--image',str(root/'first.png'),'--requested',str(marker)],'slow-image.log')
elif op=='slow_requested':output=(root/'slow-image-requested').exists()
elif op=='lossless_image':
    import gi;gi.require_version('GdkPixbuf','2.0');from gi.repository import GdkPixbuf
    image=root/(request['image']+'.png');jpeg=root/'lossless-fallback.jpg'
    assert GdkPixbuf.Pixbuf.new_from_file(str(image)).savev(str(jpeg),'jpeg',['quality'],['70'])
    output=spawn(['python3',str(root/'lossless-image-owner.py'),'--image',str(image),'--jpeg',str(jpeg),
                  '--metadata-delay',str(request.get('metadata_delay',0)),'--png-delay','0.8',
                  '--refuse-png-count',str(request.get('refuse_png_count',0)),
                  '--log',str(root/'lossless-requests.log')],'lossless-owner.log')
elif op=='handoff':
    marker=root/'handoff-requested';marker.unlink(missing_ok=True);marker.with_suffix('.ack').unlink(missing_ok=True)
    output=spawn(['python3',str(root/'slow-image-owner.py'),'--image',str(root/(request.get('image','second')+'.png')),'--requested',str(marker),'--delay',str(request.get('delay',0)),'--save-targets'],'handoff.log')
elif op=='handoff_state':output={'requested':(root/'handoff-requested').exists(),'acknowledged':(root/'handoff-requested').with_suffix('.ack').exists()}
elif op=='ui_windows':
    r=user(['xwininfo','-root','-tree'],env=env());output=r.stdout.decode()
elif op=='pastes':
    try:output=json.loads((root/'pastes.json').read_text())
    except (FileNotFoundError,json.JSONDecodeError):output=[]
elif op=='transport_metrics':
    import re
    log=(root/'agent.log').read_text(errors='replace')
    log=re.sub(r'\x1b\[[0-9;]*m','',log)
    output={'fragmented_started':log.count('peer clipboard fragmented transfer started'),
            'fragmented_finished':log.count('peer clipboard fragmented transfer finished'),
            'send_failures':log.count('send failed or timed out'),
            'sessions_ended':log.count('peer session ended'),
            'fragment_sessions':log.count('clipboard_fragments=true'),
            'legacy_sessions':log.count('clipboard_fragments=false')}
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
elif op=='screens':
    def x(*arguments):
        r=user(['xrandr',*arguments],env=env());assert r.returncode==0,r.stderr.decode();return r
    if request['mode']=='dock':
        r=user(['xrandr','--newmode','1920x1080-lab','173.00','1920','2048','2248','2576','1080','1083','1088','1120','-hsync','+vsync'],env=env())
        x('--addmode','DUMMY1','1920x1080-lab')
        x('--output','DUMMY0','--mode','1600x900-test','--pos','0x0','--primary','--output','DUMMY1','--mode','1920x1080-lab','--pos','1600x0','--fb','3520x1080')
    elif request['mode']=='small':
        r=user(['xrandr','--newmode','1366x768-lab','85.25','1366','1440','1576','1784','768','771','781','798','-hsync','+vsync'],env=env())
        x('--addmode','DUMMY0','1366x768-lab')
        x('--output','DUMMY0','--mode','1366x768-lab','--pos','0x0','--primary','--fb','1366x768')
    else:
        x('--output','DUMMY1','--off','--output','DUMMY0','--mode','1600x900-test','--pos','0x0','--primary','--fb','1600x900')
    output=x('--listmonitors').stdout.decode()
elif op=='status':
    try:output=json.loads((root/'agent.status.json').read_text())
    except (FileNotFoundError,json.JSONDecodeError):output={}
elif op=='ui_snapshot':
    code="import gi,sys;gi.require_version('Gdk','3.0');from gi.repository import Gdk;w=Gdk.get_default_root_window();p=Gdk.pixbuf_get_from_window(w,0,0,w.get_width(),w.get_height());p.savev(sys.argv[1],'png',[],[])"
    r=user(['python3','-c',code,str(root/'ui.png')],env=env());assert r.returncode==0;output=str(root/'ui.png')
elif op=='ui_open':
    r=user([str(root/'candidate-agent'),'--config',str(root/'agent.toml'),'--open-window'],env=env());assert r.returncode==0
    ui_window();output=True
elif op=='ui_click':
    window=ui_window()
    r=user(['xdotool','windowraise',window,'windowfocus','--sync',window,'mousemove','--window',window,str(request['x']),str(request['y']),'click','1'],env=env());assert r.returncode==0;output=True
elif op=='ui_drag':
    window=ui_window()
    r=user(['xdotool','windowraise',window,'windowfocus','--sync',window,'mousemove','--window',window,str(request['from_x']),str(request['from_y']),'mousedown','1','sleep','0.15','mousemove','--window',window,str(request['to_x']),str(request['to_y']),'sleep','0.2','mouseup','1'],env=env());assert r.returncode==0;output=True
elif op=='saved_layout':output=json.loads((root/'agent.topology.json').read_text())
elif op=='resources':
    pids=user(['pgrep','-f','^'+str(root)+'/candidate-agent ']).stdout.decode().split();assert len(pids)==1
    stat=Path('/proc/'+pids[0]+'/stat').read_text().rsplit(')',1)[1].split()
    output={'at':time.monotonic(),'cpu_seconds':(int(stat[11])+int(stat[12]))/os.sysconf('SC_CLK_TCK'),'rss_kib':int(stat[21])*os.sysconf('SC_PAGE_SIZE')//1024,'pid':int(pids[0])}
elif op=='private_configuration_hash':output=digest(root/'agent.toml')
elif op=='install_private_candidate':
    assert str(root).startswith('/tmp/poolsync-no-hub-')
    assert not user(['pgrep','-f','^'+str(root)+'/candidate-agent ']).stdout
    assert digest(root/'candidate-agent')==request['previous_sha256']
    assert digest(root/'candidate-agent-next')==request['candidate_sha256']
    (root/'candidate-agent-next').chmod(0o755)
    os.replace(root/'candidate-agent-next',root/'candidate-agent');output=True
elif op=='browser':
    profile=root/'browser-profile';profile.mkdir(mode=0o700)
    (profile/'user.js').write_text('user_pref("browser.shell.checkDefaultBrowser",false);user_pref("browser.startup.homepage_override.mstone","ignore");user_pref("browser.aboutwelcome.enabled",false);user_pref("datareporting.policy.firstRunURL","");')
    subprocess.run(['chown','-R','zaza:zaza',str(profile)],check=True)
    browser_port=(root/'browser-port').read_text()
    server=spawn(['python3',str(root/'browser-paste-server.py'),'--html',str(root/'clipboard-paste.html'),'--log',str(root/'browser-pastes.json'),'--port',browser_port],'browser-server.log')
    time.sleep(.3)
    engine=(root/'browser-engine').read_text()
    if engine=='firefox':
        command=['firefox','--no-remote','--profile',str(profile),'--new-window','http://127.0.0.1:'+browser_port+'/']
    else:
        assert engine=='chromium'
        # Chromium runs as the test user inside the disposable desktop only.
        # Its sandbox flag is private to this lab process, never production.
        command=['chromium','--no-sandbox','--no-first-run','--disable-background-networking',
                 '--disable-component-update','--disable-sync','--user-data-dir='+str(profile),
                 '--new-window','http://127.0.0.1:'+browser_port+'/']
    if (root/'browser-x11-trace').exists():
        # Diagnostic proxy only: zero displayed list elements suppress image
        # bytes. All applications/data remain confined to the private desktop.
        command=['xtrace','-d',env()['DISPLAY'],'-D',':'+str(int(env()['DISPLAY'][1:])+1),'-n','-m','0','-b','-o',str(root/'browser-x11.log'),'--',*command]
    if (root/'browser-syscall-trace').exists():
        # Trace the native main process without proxying its X11 connection.
        # -s 0 prevents recording any read/write payload, including clipboard.
        command=['strace','-ttt','-T','-s','0','-e','trace=poll,ppoll,read,write,writev,recvmsg,recvfrom','-o',str(root/'browser-syscall.log'),'--',*command]
    browser=spawn(command,'browser.log')
    deadline=time.monotonic()+8
    while True:
        r=user(['xdotool','search','--name','PoolSync clipboard paste test'],env=env())
        if r.returncode==0 or time.monotonic()>=deadline:break
        time.sleep(.15)
    assert r.returncode==0
    (root/'browser-window-id').write_text(r.stdout.decode().splitlines()[-1])
    output={'server':server,'browser':browser}
elif op=='browser_paste':
    # Keep the actual receiver window ID across asynchronous paste processing.
    # The page's received-pixel report remains the proof of a real paste.
    window=(root/'browser-window-id').read_text()
    r=user(['xdotool','getwindowname',window],env=env())
    if r.returncode:
        diagnostic=user(['xwininfo','-root','-tree'],env=env()).stdout
        (root/'browser-window-failure.log').write_bytes(diagnostic)
    assert r.returncode==0, 'native receiver window disappeared'
    r=user(['xdotool','windowfocus','--sync',window],env=env());assert r.returncode==0
    r=user(['xdotool','key','ctrl+v'],env=env());assert r.returncode==0;output=True
elif op=='browser_records':
    try:output=json.loads((root/'browser-pastes.json').read_text())
    except (FileNotFoundError,json.JSONDecodeError):output=[]
elif op=='browser_diagnostics':
    try:output=json.loads((root/'browser-pastes-diagnostics.json').read_text())
    except (FileNotFoundError,json.JSONDecodeError):output=[]
elif op=='key':
    r=user(['xdotool',request.get('action','key'),request['key']],env=env());assert r.returncode==0;output=True
elif op=='motion':
    # libxdo mousemove_relative warps root coordinates. Use an actual relative
    # XTEST device event so XI2 capture receives motion independently of warps.
    code="""import ctypes,os,sys,time
x=ctypes.CDLL('libX11.so.6');t=ctypes.CDLL('libXtst.so.6')
x.XOpenDisplay.argtypes=[ctypes.c_char_p];x.XOpenDisplay.restype=ctypes.c_void_p
x.XSync.argtypes=[ctypes.c_void_p,ctypes.c_int];x.XCloseDisplay.argtypes=[ctypes.c_void_p]
t.XTestFakeRelativeMotionEvent.argtypes=[ctypes.c_void_p,ctypes.c_int,ctypes.c_int,ctypes.c_ulong]
d=x.XOpenDisplay(os.environ['DISPLAY'].encode());assert d
steps=int(sys.argv[3]);interval=float(sys.argv[4]);assert 1<=steps<=250 and 0<=interval<=.02
for step in range(steps):
    sign=1 if step%2==0 else -1
    assert t.XTestFakeRelativeMotionEvent(d,int(sys.argv[1])*sign,int(sys.argv[2])*sign,0)
    x.XSync(d,0)
    if interval:time.sleep(interval)
x.XCloseDisplay(d)"""
    r=user(['python3','-c',code,str(request['dx']),str(request['dy']),str(request.get('steps',1)),str(request.get('interval',0))],env=env());assert r.returncode==0;output=True
elif op=='key_window':
    code="import gi,json,sys,time;gi.require_version('Gtk','3.0');from gi.repository import Gtk,GLib;w=Gtk.Window(title='PoolSync remote keyboard receiver');w.connect('key-press-event',lambda _,event: (open(sys.argv[1],'w').write(json.dumps({'keyval':event.keyval,'at':time.time()})),False)[1]);w.show_all();GLib.timeout_add_seconds(7200,lambda: (Gtk.main_quit(),False)[1]);Gtk.main()"
    marker=root/'remote-key.json';marker.unlink(missing_ok=True)
    output=spawn(['python3','-c',code,str(marker)],'remote-key.log')
    for _ in range(50):
        r=user(['xdotool','search','--name','^PoolSync remote keyboard receiver$'],env=env())
        if r.returncode==0:break
        time.sleep(.1)
    assert r.returncode==0
    window=r.stdout.decode().splitlines()[-1];(root/'key-window').write_text(window)
    r=user(['xdotool','windowfocus','--sync',window],env=env());assert r.returncode==0
elif op=='received_key':
    try:output=json.loads((root/'remote-key.json').read_text())
    except (FileNotFoundError,json.JSONDecodeError):output={}
elif op=='key_window_focus':
    # Keep the actual receiver identity across browser window changes. Mapping
    # it is a native window operation, never a fabricated clipboard event.
    window=(root/'key-window').read_text()
    r=user(['xwininfo','-id',window],env=env())
    if r.returncode:
        (root/'key-window-failure.log').write_bytes(user(['xwininfo','-root','-tree'],env=env()).stdout)
    assert r.returncode==0, 'native keyboard receiver window disappeared'
    r=user(['xdotool','windowmap','--sync',window,'windowraise',window,'windowfocus','--sync',window],env=env());assert r.returncode==0;output=True
elif op=='input_diagnostics':
    focused=user(['xdotool','getwindowfocus'],env=env())
    output={'focused_window':focused.stdout.decode().strip()}
    try:output['received_key']=json.loads((root/'remote-key.json').read_text())
    except (FileNotFoundError,json.JSONDecodeError):output['received_key']={}
    try:output['lease']=json.loads((root/'agent.status.json').read_text()).get('lease')
    except (FileNotFoundError,json.JSONDecodeError):output['lease']=None
elif op=='peer_connections':
    output=[]
    for line in Path('/proc/net/tcp').read_text().splitlines()[1:]:
        fields=line.split()
        if fields[3]!='01':continue
        endpoints=[field.split(':') for field in fields[1:3]]
        if not any(int(port,16) in request.get('ports',[request.get('port')]) for _,port in endpoints):continue
        output.append([socket.inet_ntoa(bytes.fromhex(address)[::-1]) for address,_ in endpoints])
elif op=='key_state':
    code="import ctypes,os,sys,json;l=ctypes.CDLL('libX11.so.6');l.XOpenDisplay.argtypes=[ctypes.c_char_p];l.XOpenDisplay.restype=ctypes.c_void_p;l.XStringToKeysym.argtypes=[ctypes.c_char_p];l.XStringToKeysym.restype=ctypes.c_ulong;l.XKeysymToKeycode.argtypes=[ctypes.c_void_p,ctypes.c_ulong];l.XKeysymToKeycode.restype=ctypes.c_ubyte;l.XQueryKeymap.argtypes=[ctypes.c_void_p,ctypes.c_void_p];l.XCloseDisplay.argtypes=[ctypes.c_void_p];d=l.XOpenDisplay(os.environ['DISPLAY'].encode());assert d;k=l.XKeysymToKeycode(d,l.XStringToKeysym(sys.argv[1].encode()));m=ctypes.create_string_buffer(32);l.XQueryKeymap(d,m);l.XCloseDisplay(d);print(json.dumps(bool(m.raw[k//8]&(1<<(k%8)))))"
    r=user(['python3','-c',code,request['key']],env=env());assert r.returncode==0;output=json.loads(r.stdout)
elif op=='button_state':
    r=user(['xinput','query-state','Virtual core XTEST pointer'],env=env());assert r.returncode==0
    output=('button['+str(request['button'])+']=down') in r.stdout.decode()
elif op=='injected_marker':
    r=user(['xprop','-root','_POOLSYNC_INJECTED_V1_'+request['node']],env=env());assert r.returncode==0
    output=' = ' in r.stdout.decode()
elif op=='local_key':
    code="import gi,json,sys;gi.require_version('Gtk','3.0');from gi.repository import Gtk,GLib;w=Gtk.Window(title='PoolSync isolated local input proof');w.connect('key-press-event',lambda _,event: (open(sys.argv[1],'w').write(json.dumps({'keyval':event.keyval})),False)[1]);w.show_all();GLib.timeout_add_seconds(8,lambda: (Gtk.main_quit(),False)[1]);Gtk.main()"
    marker=root/'key.json';marker.unlink(missing_ok=True)
    pid=spawn(['python3','-c',code,str(marker)],'key.log')
    try:
        deadline=time.monotonic()+5
        while True:
            r=user(['xdotool','search','--name','^PoolSync isolated local input proof$'],env=env())
            if r.returncode==0 or time.monotonic()>=deadline:break
            time.sleep(.1)
        assert r.returncode==0
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
    output={'listener':listener,'tls_listener':'(wss)' in log,'hub_reachable':hub,'hub_connected_logged':'connected to hub' in log,'isolated_environment':isolated,'mesh_links':log.count('peer mesh connecté'),'panic':'thread ' in log and 'panicked' in log,
            'executing_sha256':digest('/proc/'+pids[0]+'/exe') if len(pids)==1 else None}
elif op=='restore':
    original=request['original'];cfg=Path('/home/zaza/.config/poolsync/agent.toml')
    assert (digest(cfg) if cfg.exists() else None)==original['config_sha256'],'original configuration changed'
    marker=cfg.with_suffix('.away');assert (marker.read_text() if marker.exists() else None)==original['away'],'original participation changed'
    # The isolated executable is named candidate-agent, not poolsync-agent.
    # Check its exact path instead of treating a negative name lookup as cleanup.
    for process in Path('/proc').glob('[0-9]*/cmdline'):
        try:command=process.read_bytes().split(b'\0')[0]
        except (FileNotFoundError,PermissionError,ProcessLookupError):continue
        assert command!=str(root/'candidate-agent').encode(),'isolated candidate remains running'
    existing=run(['pgrep','-u','zaza','-x','poolsync-agent']).stdout.decode().split()
    if not existing and not original.get('fresh'):
        subprocess.Popen(['runuser','-m','-u','zaza','--',*original['args']],env=original['environment'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
        time.sleep(1)
    pids=run(['pgrep','-u','zaza','-x','poolsync-agent']).stdout.decode().split()
    if original.get('fresh'):
        assert not pids,'a candidate remains running in the disposable desktop'
        preserved=True
    else:
        assert len(pids)==1
        assert digest('/proc/'+pids[0]+'/exe')==original['running_sha256'],'restored agent differs'
        preserved=int(pids[0])==original['pid']
    output={'config_preserved':True,'original_agent_restored':True,'original_pid_preserved':preserved,'away_preserved':True,'candidate_agent_stopped':True,'fresh_desktop':original.get('fresh',False)}
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
    parser.add_argument('--three-kvm-peers', action='store_true', help='Make C KVM-capable and verify A controls C through B and returns')
    parser.add_argument('--kvm-soak-seconds', type=int, default=0, help='Measure active native motion/key capture with repeated software warps')
    parser.add_argument('--target-restart', action='store_true', help='Restart the actual target agent while a remote modifier/button is held')
    parser.add_argument('--peer-rate-mbit', type=float, default=0, help='Limit each private peer TCP direction through disposable per-desktop proxies')
    parser.add_argument('--peer-tls', action='store_true', help='Use private test CA/certificates and process-local trust for native TLS peer connections')
    parser.add_argument('--legacy-peer-c', type=Path, help='Qualify negotiated fallback against an existing legacy C agent binary')
    parser.add_argument('--expected-legacy-sha256', help='Required exact binary hash when --legacy-peer-c is used')
    parser.add_argument('--rolling-upgrade-from', type=Path, help='Start all three peers on an older binary, pause the whole private pool and replace peers sequentially')
    parser.add_argument('--expected-starting-sha256', help='Exact starting executable required by --rolling-upgrade-from')
    parser.add_argument('--mixed-workload-seconds', type=int, default=0, help='Keep native captured input active during image/text paste transfers')
    parser.add_argument('--mixed-browser-pastes', type=int, default=1, help='Repeat native browser image paste with a window change during each mixed round')
    parser.add_argument('--native-browser-engine', choices=('firefox','chromium'), default='firefox', help='Native browser implementation exercised by real Ctrl+V')
    parser.add_argument('--mixed-diagnostic-only', action='store_true', help='Skip the already qualified clipboard primer and stop after the focused mixed-traffic diagnostic')
    parser.add_argument('--browser-x11-trace', action='store_true', help='Trace the isolated native browser via xtrace; diagnostic runs only')
    parser.add_argument('--browser-syscall-trace', action='store_true', help='Trace native main-process I/O without payload bytes; diagnostic runs only')
    parser.add_argument('--fragment-preemption', action='store_true', help='Supersede an observed partial image and leave/rejoin while a source transfer is unfinished')
    parser.add_argument('--screen-changes', action='store_true', help='Qualify private-display docking, resolution changes and undocking')
    parser.add_argument('--network-loss', action='store_true', help='Drop only candidate TCP traffic in its network namespace, then recover')
    parser.add_argument('--clipboard-races', action='store_true', help='Qualify valid BMP and a delayed image owner superseded by fresh text')
    parser.add_argument('--simultaneous-claims', action='store_true', help='Issue concurrent native ownership shortcuts on A and B repeatedly')
    parser.add_argument('--ui-only', action='store_true', help='Diagnose native layout editing before KVM recovery scenarios')
    parser.add_argument('--participation-only', action='store_true', help='Qualify immediate departure and concurrent private rejoin without the long KVM/soak sequence')
    parser.add_argument('--lossless-only', action='store_true', help='Stop after the slow metadata/PNG lossless paste regressions')
    parser.add_argument('--lossless-rounds', type=int, default=1, help='Repeat the two native slow/refused PNG regression cases')
    parser.add_argument('--bounded-allocator', action='store_true', help='Match the launcher limits for glibc arenas and large-buffer mmap allocation')
    parser.add_argument('--layout-ui', action='store_true', help='Show the running A agent configuration window for native layout qualification')
    parser.add_argument('--flameshot', action='store_true', help='Use the real Flameshot application for the fullscreen clipboard capture')
    parser.add_argument('--native-owner', choices=['xclip','gtk'], default='xclip', help='Foreground native application used to copy fixtures')
    parser.add_argument('--large-images', action='store_true', help='Include distinct full-HD PNGs and a native XFCE screenshot')
    parser.add_argument('--idle-seconds', type=int, default=0, help='Measure interval CPU and memory without copy traffic')
    parser.add_argument('--soak-seconds', type=int, default=0, help='Continue real image/text pastes and resource sampling for this duration')
    parser.add_argument('--links', choices=['chain','triangle'], default='chain', help='Direct peer routing fixture')
    parser.add_argument('--retain-failed-desktops', action='store_true', help='Keep only disposable fresh sessions alive after failure for native diagnosis')
    parser.add_argument('--fresh-desktops', action='store_true', help='Disposable desktops with no original agent or desktop session')
    parser.add_argument('--reboot-desktops', action='store_true', help='Restart the complete disposable containers in two different orders')
    args = parser.parse_args()
    assert not args.retain_failed_desktops or args.fresh_desktops, 'failure retention is limited to disposable fresh desktops'
    assert not args.kvm_only or args.hubless, '--kvm-only requires --hubless'
    assert not args.three_kvm_peers or args.hubless, '--three-kvm-peers requires --hubless'
    assert 0 <= args.kvm_soak_seconds <= 300
    assert not args.kvm_soak_seconds or args.three_kvm_peers, '--kvm-soak-seconds requires --three-kvm-peers'
    assert not args.target_restart or args.three_kvm_peers, '--target-restart requires --three-kvm-peers'
    assert 0<=args.peer_rate_mbit<=1000, '--peer-rate-mbit must be within 0..1000'
    assert not args.peer_rate_mbit or args.hubless, 'bandwidth qualification requires --hubless'
    assert 0<=args.mixed_workload_seconds<=1200
    assert 1<=args.mixed_browser_pastes<=50
    assert args.mixed_browser_pastes==1 or (args.mixed_workload_seconds and args.native_browser), 'repeated browser paste requires the mixed native browser workload'
    assert not (args.mixed_workload_seconds or args.fragment_preemption) or (args.three_kvm_peers and args.large_images and not args.kvm_only), 'mixed/fragment qualification requires three hubless KVM peers and large clipboard images'
    assert not args.fragment_preemption or args.peer_rate_mbit, 'fragment preemption requires an observed constrained transfer'
    assert not args.mixed_diagnostic_only or (args.mixed_workload_seconds and args.native_browser), 'focused mixed diagnostic requires native browser and mixed workload'
    assert not args.browser_x11_trace or args.native_browser, 'X11 browser trace requires the native browser'
    assert not args.browser_syscall_trace or (args.native_browser and not args.browser_x11_trace), 'syscall trace requires the native browser without an X11 proxy'
    assert not (args.browser_x11_trace or args.browser_syscall_trace) or args.native_browser_engine=='firefox', 'diagnostic tracing is scoped to Firefox'
    assert not args.legacy_peer_c or args.expected_legacy_sha256, 'legacy peer requires its exact expected hash'
    assert not args.legacy_peer_c or not args.peer_rate_mbit, 'legacy fallback qualification uses the normal link; constrained legacy bulk failure is already retained'
    assert not args.rolling_upgrade_from or (args.expected_starting_sha256 and args.three_kvm_peers and not args.legacy_peer_c and not args.reboot_desktops), 'cohort upgrade requires three KVM peers, an exact starting hash and no legacy/reboot fixture'
    assert not args.reboot_desktops or args.fresh_desktops, 'Reboots require disposable fresh desktops'
    assert not args.participation_only or args.hubless, '--participation-only requires --hubless'
    assert not args.lossless_only or args.clipboard_races, '--lossless-only requires --clipboard-races'
    assert args.lossless_rounds > 0, '--lossless-rounds must be positive'
    assert os.geteuid() == 0, 'Run on the disposable Podman host as root'
    actual = hashlib.file_digest(args.candidate.open('rb'), 'sha256').hexdigest()
    assert actual == args.expected_sha256, 'Unexpected candidate binary'
    if args.legacy_peer_c:
        assert hashlib.file_digest(args.legacy_peer_c.open('rb'),'sha256').hexdigest()==args.expected_legacy_sha256, 'Unexpected legacy peer binary'
    if args.rolling_upgrade_from:
        assert hashlib.file_digest(args.rolling_upgrade_from.open('rb'),'sha256').hexdigest()==args.expected_starting_sha256, 'Unexpected cohort starting binary'
    run_id = uuid.uuid4().hex[:12]
    root = '/tmp/poolsync-no-hub-' + run_id
    artifacts = args.output.parent / ('private-' + run_id)
    artifacts.mkdir(mode=0o700, parents=True)
    nodes = ['nohub-a', 'nohub-b', 'nohub-c']
    tokens = {node: secrets.token_hex(24) for node in nodes}
    e2e_key = base64.b64encode(secrets.token_bytes(32)).decode()
    port = 19676
    peer_wire_port = 19686 if args.peer_rate_mbit else port
    snapshots, agents, receivers, key_windows, images, xservers, buses, proxies = {}, {}, {}, {}, {}, {}, {}, {}
    browser_workers = {}
    network_rules = []
    checked, checks, restoration = set(), [], {}
    latencies, copies = [], []
    result = {'candidate_sha256': actual, 'production_hub_used': False, 'hub_available_during_test': False,
              'serverless_kvm': None, 'input_source': 'synthetic X11 events', 'restart_scope': 'agents',
              'isolated_xorg_display': args.display,
              'receiver_mode': 'native GTK paste handler sampled once per second',
              'checks': checks, 'paste_convergence_seconds': latencies, 'native_copy_owners': copies}
    result['harness_sha256'] = HARNESS_SHA256
    result['direct_links']=args.links
    result['three_kvm_peers']=args.three_kvm_peers
    result['fresh_desktops']=args.fresh_desktops
    result['native_copy_application']=args.native_owner
    result['allocator']={'arena_max':2,'mmap_threshold':131072} if args.bounded_allocator else 'system default'
    result['network_fixture']={'per_direction_rate_mbit':args.peer_rate_mbit,'per_peer_disposable_proxy':bool(args.peer_rate_mbit),'central_proxy':False,'partition_ports':sorted({port,peer_wire_port})}
    result['peer_tls']=args.peer_tls
    result['browser_x11_trace']=args.browser_x11_trace
    result['browser_syscall_trace']=args.browser_syscall_trace
    result['native_browser_engine']=args.native_browser_engine if args.native_browser else None
    browser_name={'firefox':'Firefox','chromium':'Chromium'}[args.native_browser_engine]
    result['qualification_scope']='focused mixed native clipboard/input diagnostic' if args.mixed_diagnostic_only else 'full selected scenarios'
    result['peer_binary_sha256']=[actual,actual,args.expected_legacy_sha256 if args.legacy_peer_c else actual]
    starting_hashes=[args.expected_starting_sha256]*3 if args.rolling_upgrade_from else result['peer_binary_sha256']
    result['initial_peer_binary_sha256']=starting_hashes
    browser_fixtures={name:Path(__file__).with_name(name).read_text() for name in ('browser-paste-server.py','clipboard-paste.html')} if args.native_browser else {}
    result['browser_fixture_sha256']={name:hashlib.sha256(contents.encode()).hexdigest() for name,contents in browser_fixtures.items()}
    resource_samples=[]
    result['resource_samples']=resource_samples

    def op(index, operation, **extra):
        request = {'root': root, 'op': operation, 'fresh':args.fresh_desktops, **extra}
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

    def network_drop(index, enabled):
        pid=subprocess.check_output(['podman','inspect',args.containers[index],'--format','{{.State.Pid}}'],text=True).strip()
        rules=[('INPUT','--dport'),('OUTPUT','--sport'),('OUTPUT','--dport')]
        for transport_port in sorted({port,peer_wire_port}):
            for chain,option in rules:
                rule=['-p','tcp',option,str(transport_port),'-m','comment','--comment','poolsync-lab-'+run_id,'-j','DROP']
                command=['nsenter','--target',pid,'--net','iptables','-I' if enabled else '-D',chain,*rule]
                subprocess.run(command,check=True,capture_output=True,timeout=5)
                item=(index,pid,chain,rule)
                if enabled:network_rules.append(item)
                else:network_rules.remove(item)

    try:
        # Reject missing native applications before changing any lab session.
        if args.native_browser:
            version=subprocess.run(['podman','exec',args.containers[1],args.native_browser_engine,'--version'],capture_output=True,text=True,timeout=10)
            assert version.returncode==0, 'Selected native browser is unavailable in desktop B'
            result['native_browser_version']=version.stdout.strip()[:256]
        if args.large_images and not args.kvm_only:
            screenshot_application='flameshot' if args.flameshot else 'xfce4-screenshooter'
            available=subprocess.run(['podman','exec',args.containers[0],'python3','-c',
                'import shutil,sys;sys.exit(0 if shutil.which(sys.argv[1]) else 1)',screenshot_application],capture_output=True,timeout=10)
            assert available.returncode==0, screenshot_application+' is unavailable in desktop A'
            result['native_screenshot_application']=screenshot_application
        tls_material={}
        if args.peer_tls:
            security=artifacts/'tls';security.mkdir(mode=0o700)
            def openssl(*argv):
                subprocess.run(['openssl',*map(str,argv)],check=True,capture_output=True,timeout=15)
            openssl('req','-x509','-newkey','rsa:2048','-nodes','-days','2','-subj','/CN=PoolSync isolated test CA',
                    '-addext','basicConstraints=critical,CA:TRUE','-addext','keyUsage=critical,keyCertSign,cRLSign',
                    '-keyout',security/'ca.key','-out',security/'ca.pem')
            (security/'ca.key').chmod(0o600)
            for index,node in enumerate(nodes):
                key=security/(node+'.key');csr=security/(node+'.csr');cert=security/(node+'.pem');extension=security/(node+'.ext')
                extension.write_text('subjectAltName=IP:'+args.addresses[index]+'\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n')
                openssl('req','-new','-newkey','rsa:2048','-nodes','-subj','/CN='+node,'-keyout',key,'-out',csr);key.chmod(0o600)
                openssl('x509','-req','-in',csr,'-CA',security/'ca.pem','-CAkey',security/'ca.key','-set_serial',str(index+1),'-days','2','-extfile',extension,'-out',cert)
                tls_material[index]={'peer-ca.pem':(security/'ca.pem').read_text(),'peer.pem':cert.read_text()+(security/'ca.pem').read_text(),'peer.key':key.read_text()}
        for index in range(3):
            snapshots[index] = op(index, 'snapshot')
        (artifacts / 'original-agents.json').write_text(json.dumps(snapshots))
        (artifacts / 'original-agents.json').chmod(0o600)
        for index, node in enumerate(nodes):
            peers = [j for j in range(3) if j != index and (args.links=='triangle' or abs(j - index) == 1)]
            full_mode = index != 2 or args.three_kvm_peers
            lines = [f'node={json.dumps(node)}', 'hub_url="ws://127.0.0.1:1/ws"', 'token="isolated-unused-hub-token"',
                     'hubless='+str(args.hubless).lower(),
                     f'node_token={json.dumps(tokens[node])}', f'e2e_key={json.dumps(e2e_key)}',
                     'mode="full"' if full_mode else 'mode="clipboard_only"',
                     'kvm_enabled=' + str(full_mode).lower(), 'kvm_capture=' + str(full_mode).lower(),
                     f'peer_listen_port={port}', 'peer_direct_clipboard=true', 'hub_clipboard=false',
                     'clipboard_poll_ms=80', 'pause_clipboard_when_rdp=false', 'input_poll_ms=8',
                     '[screen]', 'width=1600', 'height=900', '[peer_tokens]']
            if args.peer_tls:
                at=lines.index('[screen]');lines[at:at]=[f'peer_tls_cert={json.dumps(root+"/peer.pem")}',f'peer_tls_key={json.dumps(root+"/peer.key")}']
            lines += [f'{nodes[j]}={json.dumps(tokens[nodes[j]])}' for j in range(3) if j != index]
            for j in peers:
                lines += ['[[neighbors]]', f'node={json.dumps(nodes[j])}',
                          'direction="right"' if j > index else 'direction="left"',
                          f'peer_url="{"wss" if args.peer_tls else "ws"}://{args.addresses[j]}:{peer_wire_port}/ws"']
            layout={'revision':1,'origin':'nohub-a','topology':{'nodes':{name:{'x':j*1600,'y':0,'width':1600,'height':900,'kvm_enabled':j!=2 or args.three_kvm_peers} for j,name in enumerate(nodes)}}}
            images[index] = op(index, 'prepare', original=snapshots[index], config='\n'.join(lines) + '\n',slow_fixture=Path(__file__).with_name('slow-image-owner.py').read_text(),lossless_fixture=Path(__file__).with_name('lossless-image-owner.py').read_text(),bandwidth_proxy=Path(__file__).with_name('peer-bandwidth-proxy.py').read_text() if args.peer_rate_mbit else None,browser_fixtures=browser_fixtures,browser_engine=args.native_browser_engine,trace_transport=bool(args.peer_rate_mbit or args.mixed_workload_seconds or args.legacy_peer_c),tls=tls_material.get(index),show_window=args.layout_ui and index==0,large_images=args.large_images,native_owner=args.native_owner,browser_x11_trace=args.browser_x11_trace and index==1,browser_syscall_trace=args.browser_syscall_trace and index==1,bounded_allocator=args.bounded_allocator,display=args.display,xorg_config=XORG_CONFIG,layout=layout)
            peer_binary=args.rolling_upgrade_from or (args.legacy_peer_c if index==2 and args.legacy_peer_c else args.candidate)
            subprocess.run(['podman', 'cp', str(peer_binary), args.containers[index] + ':' + root + '/candidate-agent'], check=True, capture_output=True)
            subprocess.run(['podman', 'exec', args.containers[index], 'chmod', '755', root + '/candidate-agent'], check=True, capture_output=True)
        for index in range(3):
            checked.add(index)
            xservers[index]=op(index,'xserver')
            buses[index]=op(index,'dbus')
        time.sleep(1.5)
        for index in (2, 0, 1):
            if args.peer_rate_mbit:proxies[index]=op(index,'bandwidth_proxy',listen_port=peer_wire_port,backend_port=port,rate_mbit=args.peer_rate_mbit)
            agents[index] = op(index, 'start')
            receivers[index] = op(index, 'receiver')
        time.sleep(4)
        health = [op(i, 'health', port=port) for i in range(3)]
        check('Cold start: isolated agents run with hub unreachable', all(h['listener'] and h['isolated_environment'] and not h['hub_reachable'] and not h['hub_connected_logged'] for h in health))
        check('Every isolated peer executes its expected starting binary',all(h['executing_sha256']==expected for h,expected in zip(health,starting_hashes)))
        if args.peer_tls:check('Every private peer listener uses TLS with isolated credentials',all(h['tls_listener'] for h in health))
        if args.rolling_upgrade_from:
            config_hashes=[op(i,'private_configuration_hash') for i in range(3)]
            for index in range(3):op(index,'away',value=True)
            time.sleep(2)
            check('Cohort upgrade pauses every old peer before replacing any binary',all(not op(i,'status')['peers'][nodes[i]]['active'] for i in range(3)))
            check('Every paused old peer retains native local keyboard input',all(op(i,'local_key') for i in range(3)))
            private_text='Private clipboard during cohort maintenance '+uuid.uuid4().hex
            op(2,'copy',text=private_text)
            steps=[];result['rolling_upgrade']={'starting_sha256':args.expected_starting_sha256,'candidate_sha256':actual,'steps':steps}
            for index in (1,2,0):
                old_pid=agents.pop(index);op(index,'stop',pid=old_pid)
                subprocess.run(['podman','cp',str(args.candidate),args.containers[index]+':'+root+'/candidate-agent-next'],check=True,capture_output=True)
                op(index,'install_private_candidate',previous_sha256=args.expected_starting_sha256,candidate_sha256=actual)
                agents[index]=op(index,'start');deadline=time.monotonic()+15
                while time.monotonic()<deadline:
                    h=op(index,'health',port=port);status=op(index,'status')
                    if h['executing_sha256']==actual and h['listener'] and status.get('peers',{}).get(nodes[index],{}).get('active') is False:break
                    time.sleep(.1)
                else:raise AssertionError('New peer did not start in the maintained absence state')
                preserved=op(index,'private_configuration_hash')==config_hashes[index]
                check('Paused '+nodes[index]+' replaces only its private executable',preserved)
                steps.append({'node':nodes[index],'old_pid':old_pid,'new_pid':agents[index],'running_sha256':actual,'configuration_preserved':preserved,'absence_preserved':True})
                assert all(not op(i,'status')['peers'][nodes[i]]['active'] for i in range(3)), 'A mixed-version peer resumed during maintenance'
            check('The full cohort executes the new binary before participation resumes',all(op(i,'health',port=port)['executing_sha256']==actual for i in range(3)))
            for index in range(3):op(index,'away',value=False)
            deadline=time.monotonic()+15
            while time.monotonic()<deadline:
                if all(op(i,'status')['peers'][nodes[i]]['active'] for i in range(3)):break
                time.sleep(.1)
            else:raise AssertionError('Updated cohort participation did not resume')
            time.sleep(2)
            check('A private maintenance copy is not replayed when the upgraded cohort rejoins',all(op(i,'text')!=private_text for i in (0,1)))
            copy_text(0,'Fresh public copy after cohort maintenance')
            check('Fresh native clipboard delivery resumes after the cohort upgrade')
            metrics=[op(i,'transport_metrics') for i in range(3)];result['rolling_upgrade']['transport_metrics']=metrics
            check('The resumed cohort negotiates fragments on every peer',all(m['fragment_sessions']>0 for m in metrics))
        if args.legacy_peer_c:
            metrics=[op(i,'transport_metrics') for i in range(3)];result['compatibility_transport_metrics']=metrics
            check('New A/B negotiate fragments and B falls back to legacy C',metrics[0]['fragment_sessions']>0 and metrics[1]['fragment_sessions']>0 and metrics[1]['legacy_sessions']>0 and not metrics[2]['fragmented_started'])
        if args.reboot_desktops:
            for order in ((0,1,2),(1,2,0)):
                subprocess.run(['podman','stop','--time','3',*args.containers],check=True,capture_output=True,timeout=20)
                agents.clear();receivers.clear();buses.clear();xservers.clear();browser_workers.clear();proxies.clear()
                for index in order:
                    subprocess.run(['podman','start',args.containers[index]],check=True,capture_output=True,timeout=10)
                    op(index,'cold_reset')
                    xservers[index]=op(index,'xserver');buses[index]=op(index,'dbus');time.sleep(.6)
                    if args.peer_rate_mbit:proxies[index]=op(index,'bandwidth_proxy',listen_port=peer_wire_port,backend_port=port,rate_mbit=args.peer_rate_mbit)
                    agents[index]=op(index,'start');receivers[index]=op(index,'receiver')
                time.sleep(4)
                copy_text(order[-1],'Fresh clipboard after complete container cold start '+str(order))
                check('Complete container cold start restores hubless delivery in order '+str(order))
            result['restart_scope']='agents and complete disposable containers'

        if args.ui_only:
            op(0,'ui_open');time.sleep(.4)
            op(0,'ui_click',x=210,y=16);time.sleep(.6)
            before=op(0,'saved_layout')
            op(0,'ui_snapshot')
            op(0,'ui_drag',from_x=534,from_y=230,to_x=195,to_y=421);time.sleep(.5)
            op(0,'ui_click',x=355,y=62);time.sleep(3)
            saved=[op(i,'saved_layout') for i in range(3)]
            result['native_layout_document']=saved
            check('Native drag/save persists and gossips',all(d['revision']>before['revision'] and d['topology']['nodes'][nodes[1]]['x']==0 and d['topology']['nodes'][nodes[1]]['y']==900 for d in saved) and all(d==saved[0] for d in saved))
            restart(0);time.sleep(3)
            check('Native layout survives restart',all(op(i,'status')['topology']['nodes'][nodes[1]]['y']==900 for i in range(3)))
            op(0,'ui_open');time.sleep(.4);op(0,'ui_click',x=210,y=16);time.sleep(.4)
            op(0,'ui_click',x=18,y=499);time.sleep(.4);op(0,'ui_snapshot')
            op(0,'ui_click',x=30,y=530);op(0,'ui_click',x=355,y=62);time.sleep(3)
            check('Native permission edit disables B in persisted and live layouts',all(not op(i,'saved_layout')['topology']['nodes'][nodes[1]]['kvm_enabled'] and not op(i,'status')['topology']['nodes'][nodes[1]]['kvm_enabled'] for i in range(3)))
            op(0,'key',key='ctrl+alt+shift+m');time.sleep(.5);op(1,'key',key='ctrl+alt+shift+m');time.sleep(2)
            check('Pool-disabled B cannot claim control and keeps its native keyboard',all(op(i,'status').get('lease',{}).get('owner')!=nodes[1] for i in range(3)) and op(1,'local_key'))
            op(0,'ui_click',x=30,y=530);op(0,'ui_click',x=355,y=62);time.sleep(3)
            check('Native permission restore enables B without changing its position',all(op(i,'status')['topology']['nodes'][nodes[1]]['kvm_enabled'] and op(i,'saved_layout')['topology']['nodes'][nodes[1]]['y']==900 for i in range(3)))
            result['functional_checks_passed']=True
            return

        if args.participation_only:
            copy_text(0,'Initial shared clipboard before immediate departure')
            for wake_round in range(20):
                op(2,'away',value=True)
                private=copy_text(2,'Immediate private departure '+str(wake_round),targets=(2,))
                private_hash=hashlib.sha256(private.encode()).hexdigest()
                check('Acknowledged departure keeps the immediate native copy usable round '+str(wake_round))
                with ThreadPoolExecutor(max_workers=2) as executor:
                    returning=executor.submit(op,2,'away',value=False)
                    office_copy=executor.submit(copy_text,0,'Concurrent office/rejoin '+str(wake_round),targets=(0,1))
                    returning.result();office_copy.result()
                copy_text(0,'Fresh copy after acknowledged rejoin '+str(wake_round))
                check('Private contents never enter office receiver history round '+str(wake_round),all(not any(record['sha256']==private_hash for record in op(i,'pastes')) for i in (0,1)))
            result['functional_checks_passed']=True
            return

        if args.mixed_diagnostic_only:
            copy_text(0,'Shared text before focused mixed native diagnostic')
            browser_workers=op(1,'browser');time.sleep(8);result['native_browser_paste']=True
        elif not args.kvm_only:
            copy_text(0, 'Cold-start A to B to C')
            check('Encrypted native text crosses the A-B-C chain without hub')
            copy_text(2, 'Clipboard-only C to B to A')
            check('Node C sends native text in the reverse direction')
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
            check('Image after text travels from C to A')

            if args.native_browser:
                browser_workers=op(1,'browser');time.sleep(8)
                for image in ('second','first'):
                    started=time.time();op(0,'copy',image=image);expect((0,1,2),images[0][image],started)
                    op(1,'browser_paste');deadline=time.monotonic()+8
                    while time.monotonic()<deadline:
                        if any(e['at']>=started and e.get('sha256')==images[0][image]['sha256'] for e in op(1,'browser_records')):break
                        time.sleep(.2)
                    else:raise AssertionError(f'Native {browser_name} did not paste the '+image+' image')
                    check(f'Native {browser_name} Ctrl+V pastes the '+image+' image with matching RGBA pixels')
                text=copy_text(0,'Native browser text after screenshots');started=time.time();op(1,'browser_paste');deadline=time.monotonic()+8
                while time.monotonic()<deadline:
                    if any(e['at']>=started and e.get('sha256')==hashlib.sha256(text.encode()).hexdigest() for e in op(1,'browser_records')):break
                    time.sleep(.2)
                else:raise AssertionError(f'Native {browser_name} did not paste the fresh text')
                check(f'Native {browser_name} Ctrl+V pastes text after images')
                started=time.time();op(0,'copy',image='first');expect((0,1,2),images[0]['first'],started);op(1,'browser_paste');deadline=time.monotonic()+8
                while time.monotonic()<deadline:
                    if any(e['at']>=started and e.get('sha256')==images[0]['first']['sha256'] for e in op(1,'browser_records')):break
                    time.sleep(.2)
                else:raise AssertionError(f'Native {browser_name} did not paste an image after text')
                check(f'Native {browser_name} Ctrl+V pastes an image after text')
                result['native_browser_paste']=True

            if args.large_images:
                for image in ('large-first','large-second'):
                    started=time.time();op(0,'copy',image=image);expect((0,1,2),images[0][image],started)
                    check('Native GTK pastes the distinct full-HD '+image+' PNG')
                    if args.native_browser:
                        op(1,'browser_paste');deadline=time.monotonic()+10
                        while time.monotonic()<deadline:
                            if any(e['at']>=started and e.get('sha256')==images[0][image]['sha256'] for e in op(1,'browser_records')):break
                            time.sleep(.2)
                        else:raise AssertionError(f'Native {browser_name} did not paste a full-HD PNG')
                        check(f'Native {browser_name} pastes the full-HD '+image+' PNG')
                copy_text(0,'Native text following a full-HD image')
                started=time.time();screenshot=op(0,'screenshot',app='flameshot' if args.flameshot else 'xfce');expect((0,1,2),screenshot,started)
                check('A real '+('Flameshot' if args.flameshot else 'XFCE')+' screenshot survives application close and pastes on every peer')

            if args.clipboard_races:
                lossless_cases=[(label+' round '+str(round_), delay, refused) for round_ in range(args.lossless_rounds) for label,delay,refused in [('slow metadata',0.45,0),('temporary PNG refusal',0,5)]]
                for label, metadata_delay, refused in lossless_cases:
                    copy_text(0,'Distinct clipboard baseline before '+label)
                    started=time.time();owner=op(0,'lossless_image',image='first',metadata_delay=metadata_delay,refuse_png_count=refused)
                    try:
                        expect((0,1,2),images[0]['first'],started)
                        check('Lossless PNG pixels survive '+label+' with JPEG also advertised')
                        if args.native_browser:
                            op(1,'browser_paste');deadline=time.monotonic()+10
                            while time.monotonic()<deadline:
                                if any(e['at']>=started and e.get('sha256')==images[0]['first']['sha256'] for e in op(1,'browser_records')):break
                                time.sleep(.2)
                            else:raise AssertionError(f'Native {browser_name} did not paste the lossless image')
                            check(f'Native {browser_name} preserves lossless pixels after '+label)
                        copy_text(0,'Fresh native copy ends '+label)
                    finally:op(0,'stop',pid=owner)
                if args.lossless_only:
                    check('Hub remained unreachable during lossless regressions',all(not op(i,'health',port=port)['hub_connected_logged'] for i in range(3)))
                    result['functional_checks_passed']=True
                    return
                for image in ('second','first'):
                    started=time.time();op(1,'copy',image=image,bmp=True)
                    expect((0,1,2),images[1][image],started)
                    check('A genuine new '+image+' BMP supersedes the cached PNG')
                    if args.native_browser:
                        op(1,'browser_paste');deadline=time.monotonic()+8
                        while time.monotonic()<deadline:
                            if any(e['at']>=started and e.get('sha256')==images[1][image]['sha256'] for e in op(1,'browser_records')):break
                            time.sleep(.2)
                        else:raise AssertionError(f'Native {browser_name} did not paste the converted BMP')
                        check(f'Native {browser_name} pastes the fresh converted '+image+' BMP')
                started=time.time();handoff=op(0,'handoff')
                expect((0,1,2),images[0]['second'],started)
                deadline=time.monotonic()+5
                while not op(0,'handoff_state')['acknowledged'] and time.monotonic()<deadline:time.sleep(.1)
                check('Application SAVE_TARGETS handoff preserves a fresh image after close',op(0,'handoff_state')['acknowledged'])
                copy_text(0,'Native copy after application handoff')
                delayed=op(0,'handoff',delay=6)
                try:
                    deadline=time.monotonic()+5
                    while not op(0,'handoff_state')['requested'] and time.monotonic()<deadline:time.sleep(.1)
                    check('Delayed application handoff starts a real image transfer',op(0,'handoff_state')['requested'])
                    fresh_handoff=copy_text(0,'Fresh text supersedes delayed SAVE_TARGETS')
                    time.sleep(7)
                    check('Delayed application handoff cannot replace a newer native copy',all(op(i,'text')==fresh_handoff for i in range(3)))
                finally:op(0,'stop',pid=delayed)
                slow=op(0,'slow_image')
                try:
                    deadline=time.monotonic()+8
                    while not op(0,'slow_requested') and time.monotonic()<deadline:time.sleep(.1)
                    check('The delayed native image owner has received a real read request',op(0,'slow_requested'))
                    started=time.time();fresh=copy_text(0,'Fresh text supersedes an unfinished image read')
                    result['superseded_image_text_seconds']=round(time.time()-started,3)
                    check('Fresh text converges before the six-second image response',time.time()-started<5)
                    time.sleep(7)
                    check('A late image response cannot restore the previous capture',all(op(i,'text')==fresh for i in range(3)))
                finally:op(0,'stop',pid=slow)

        if args.hubless:
            check('All three peers discover each other through the chain',all(len(op(i,'status').get('peers',{}))==3 for i in range(3)))
            key_windows[1]=op(1,'key_window')
            if args.three_kvm_peers:key_windows[2]=op(2,'key_window')
            op(1,'move',x=300,y=250);width,height=op(0,'screen')
            op(0,'move',x=width//2,y=height//2);op(0,'key',key='ctrl+alt+shift+m');time.sleep(.5)
            op(0,'move',x=width-1,y=height//2)
            wait_lease(0,1)
            result['kvm_entry_pointer']=op(1,'pointer')
            check('A crosses to B with peer-controlled ownership and no hub',result['kvm_entry_pointer']['x']<100)
            before=op(1,'pointer')
            for x in (30,width-30)*3:
                op(0,'move',x=x,y=height//2);time.sleep(.2)
            time.sleep(.65)
            lease=op(0,'status').get('lease',{})
            check('Software pointer warps during A capture neither move B nor bounce its focus',
                  op(1,'pointer')==before and lease.get('owner')==nodes[0] and lease.get('focus')==nodes[1])
            before=op(1,'pointer');op(0,'motion',dx=70,dy=35);time.sleep(.4)
            check('Grabbed A motion moves the actual B pointer',op(1,'pointer')!=before)
            key_started=time.time();op(0,'key',key='z');time.sleep(.4)
            check('A keyboard reaches the real B GTK window',op(1,'received_key').get('keyval')==ord('z'))
            result['native_kvm_key_seconds']=round(op(1,'received_key')['at']-key_started,4)
            if args.three_kvm_peers:
                result['native_control_connections']=[op(i,'peer_connections',ports=[port,peer_wire_port]) for i in range(3)]
                if args.links=='chain':
                    check('Relay fixture has no direct candidate TCP connection between A and C',
                          all(args.addresses[2] not in connection for connection in result['native_control_connections'][0])
                          and all(args.addresses[0] not in connection for connection in result['native_control_connections'][2]))
                time.sleep(.65);op(0,'motion',dx=1600,dy=0);wait_lease(0,2,indices=(0,1,2))
                check('A crosses through B to the actual C desktop without a hub',op(2,'pointer')['x']<100)
                op(0,'key',key='x');time.sleep(.4)
                check('A keyboard reaches the real C GTK window through the peer mesh',op(2,'received_key').get('keyval')==ord('x'))
                before=op(2,'pointer');op(0,'motion',dx=70,dy=25);time.sleep(.4)
                check('Captured A relative motion moves C through the peer mesh',op(2,'pointer')!=before)
                if args.mixed_workload_seconds:
                    baseline=op(0,'status')['lease'];before_resources=[op(i,'resources') for i in range(3)]
                    mixed_started=time.monotonic();rounds=0;key_latencies=[]
                    def pulse_until(future):
                        while not future.done():
                            op(0,'motion',dx=2,dy=1,steps=24,interval=.008)
                            sent=time.time();op(0,'key',key='h');deadline=time.monotonic()+2
                            while time.monotonic()<deadline:
                                received=op(2,'received_key')
                                if received.get('keyval')==ord('h') and received.get('at',0)>=sent:break
                                time.sleep(.04)
                            else:
                                result['mixed_failure']={'sent_at':sent,'observation_at':time.time(),
                                    'native_key_latency_seconds':key_latencies,
                                    'input_diagnostics':[op(i,'input_diagnostics') for i in range(3)],
                                    'transport_metrics':[op(i,'transport_metrics') for i in range(3)]}
                                raise AssertionError('Native C input stopped during a clipboard transfer')
                            key_latencies.append(round(received['at']-sent,4))
                            assert all(op(i,'status').get('lease')==baseline for i in range(3)), 'Clipboard transfer changed or expired the active input lease'
                        future.result()
                    while time.monotonic()-mixed_started<args.mixed_workload_seconds:
                        source=0 if rounds%2==0 else 2;image='large-first' if rounds%2==0 else 'large-second'
                        since=time.time();op(source,'copy',image=image)
                        with ThreadPoolExecutor(max_workers=1) as executor:
                            pulse_until(executor.submit(expect,(0,1,2),images[source][image],since))
                        if args.native_browser:
                            def paste_mixed_image():
                                for paste_round in range(args.mixed_browser_pastes):
                                    if args.mixed_browser_pastes>1:op(1,'key_window_focus')
                                    paste_since=time.time();op(1,'browser_paste');deadline=time.monotonic()+10
                                    while time.monotonic()<deadline:
                                        if any(e['at']>=paste_since and e.get('sha256')==images[source][image]['sha256'] for e in op(1,'browser_records')):break
                                        time.sleep(.2)
                                    else:
                                        result['browser_failure']={'requested_after':paste_since,'mixed_round':rounds,'paste_round':paste_round,
                                            'records':op(1,'browser_records'),'diagnostics':op(1,'browser_diagnostics'),
                                            'input_diagnostics':[op(i,'input_diagnostics') for i in range(3)]}
                                        raise AssertionError('Browser image paste failed while C control remained active')
                            with ThreadPoolExecutor(max_workers=1) as executor:
                                pulse_until(executor.submit(paste_mixed_image))
                        with ThreadPoolExecutor(max_workers=1) as executor:
                            pulse_until(executor.submit(copy_text,1,'Fresh text during active KVM round '+str(rounds)))
                        rounds+=1
                        print(json.dumps({'mixed_progress':{'elapsed_seconds':round(time.monotonic()-mixed_started,1),'image_text_rounds':rounds,'fresh_native_keys':len(key_latencies),'maximum_native_key_seconds':max(key_latencies) if key_latencies else None}}),flush=True)
                    after_resources=[op(i,'resources') for i in range(3)]
                    result['mixed_workload']={'elapsed_seconds':round(time.monotonic()-mixed_started,3),'large_image_text_rounds':rounds,
                        'native_browser_pastes_per_round':args.mixed_browser_pastes if args.native_browser else 0,
                        'native_key_latency_seconds':key_latencies,'unchanged_lease':baseline,
                        'transport_metrics':[op(i,'transport_metrics') for i in range(3)],
                        'resources':[{'node':node,'cpu_percent':100*(b['cpu_seconds']-a['cpu_seconds'])/(b['at']-a['at']),'rss_start_kib':a['rss_kib'],'rss_end_kib':b['rss_kib'],'pid_preserved':a['pid']==b['pid']} for node,a,b in zip(nodes,before_resources,after_resources)]}
                    check('Large image/text traffic preserves active C control and fresh native GTK keys',bool(key_latencies) and max(key_latencies)<1 and all(a['pid']==b['pid'] for a,b in zip(before_resources,after_resources)))
                    if args.native_browser:check(f'Native {browser_name} pastes the mixed-workload images while C remains controlled')
                    if args.mixed_diagnostic_only:
                        result['functional_checks_passed']=True;result['serverless_kvm']=True
                        return
                if args.fragment_preemption:
                    def start_partial(index,image):
                        baseline=op(index,'transport_metrics');since=time.time();op(index,'copy',image=image);deadline=time.monotonic()+8
                        while time.monotonic()<deadline:
                            metrics=op(index,'transport_metrics')
                            if metrics['fragmented_started']>baseline['fragmented_started'] and metrics['fragmented_finished']==baseline['fragmented_finished']:return since
                            time.sleep(.03)
                        raise AssertionError('The large-copy transfer was not observed unfinished')
                    since=start_partial(0,'large-first')
                    fresh=copy_text(0,'Fresh text cancels the partial remote image');time.sleep(8)
                    check('A superseded partial image never reaches either remote GTK paste history',all(not any(p['at']>=since and p['sha256']==images[0]['large-first']['sha256'] for p in op(i,'pastes')) for i in (1,2)))
                    check('Fresh text remains selected after the cancelled image drains',all(op(i,'text')==fresh for i in range(3)))
                    since=start_partial(2,'large-first');op(2,'away',value=True)
                    private=copy_text(2,'Private text after cancelling a source transfer',targets=(2,));private_hash=hashlib.sha256(private.encode()).hexdigest()
                    op(2,'away',value=False);time.sleep(8)
                    check('Departure cancels the source image without publishing private contents on return',all(not any(p['at']>=since and p['sha256'] in (private_hash,images[2]['large-first']['sha256']) for p in op(i,'pastes')) for i in (0,1)))
                    copy_text(0,'Fresh shared text after cancelled-transfer rejoin')
                    op(0,'move',x=width//2,y=height//2);op(0,'key',key='ctrl+alt+shift+m');time.sleep(.8)
                    op(0,'move',x=width-1,y=height//2);wait_lease(0,1)
                    time.sleep(.65);op(0,'motion',dx=1600,dy=0);wait_lease(0,2,indices=(0,1,2))
                    op(0,'motion',dx=70,dy=0);time.sleep(.65)
                if args.kvm_soak_seconds:
                    resources_before=[op(i,'resources') for i in range(3)]
                    started=time.monotonic();bursts=0;key_latencies=[]
                    while time.monotonic()-started<args.kvm_soak_seconds:
                        op(0,'move',x=30 if bursts%2 else width-30,y=height//2)
                        op(0,'motion',dx=2,dy=1,steps=124,interval=.008)
                        leases=[op(i,'status').get('lease',{}) for i in range(3)]
                        assert all(l.get('owner')==nodes[0] and l.get('focus')==nodes[2] for l in leases), 'Active capture bounced focus during native motion/warp soak'
                        if bursts%5==0:
                            key_started=time.time();op(0,'key',key='d');deadline=time.monotonic()+2
                            while time.monotonic()<deadline:
                                received=op(2,'received_key')
                                if received.get('keyval')==ord('d') and received.get('at',0)>=key_started:break
                                time.sleep(.05)
                            else:raise AssertionError('C GTK missed fresh input during active capture soak')
                            key_latencies.append(round(received['at']-key_started,4))
                        bursts+=1
                    resources_after=[op(i,'resources') for i in range(3)]
                    result['active_kvm_soak']={'elapsed_seconds':round(time.monotonic()-started,3),'motion_events':bursts*124,'software_warps':bursts,'native_key_latency_seconds':key_latencies,
                        'resources':[{'node':node,'cpu_percent':100*(b['cpu_seconds']-a['cpu_seconds'])/(b['at']-a['at']),'rss_start_kib':a['rss_kib'],'rss_end_kib':b['rss_kib'],'pid_preserved':a['pid']==b['pid']} for node,a,b in zip(nodes,resources_before,resources_after)]}
                    check('Sustained native device motion and software warps retain C focus and GTK input',all(a['pid']==b['pid'] for a,b in zip(resources_before,resources_after)))
                if args.network_loss:
                    lease_before_partition=op(0,'status')['lease']
                    op(0,'key',action='keydown',key='Shift_L');time.sleep(.4)
                    check('A held modifier reaches the actual C desktop',op(2,'key_state',key='Shift_L'))
                    network_drop(1,True);time.sleep(5)
                    if args.links=='chain':
                        check('Relay partition releases the held modifier on C',not op(2,'key_state',key='Shift_L'))
                        # The source fixture still physically holds Shift.
                        # Release it before the lowercase local GTK assertion.
                        op(0,'key',action='keyup',key='Shift_L')
                        check('Relay partition recovers native input on both endpoints',op(0,'local_key') and op(2,'local_key'))
                    else:
                        wait_lease(0,2,indices=(0,2))
                        check('Losing B preserves A control of C over the alternate direct path',op(2,'key_state',key='Shift_L'))
                        op(0,'key',action='keyup',key='Shift_L');time.sleep(.2)
                        op(0,'key',key='y');time.sleep(.4)
                        check('C native GTK still receives A input while B is unreachable',op(2,'received_key').get('keyval')==ord('y'))
                        op(0,'key',action='keydown',key='Shift_L');time.sleep(.2)
                    network_drop(1,False);time.sleep(4)
                    if args.links=='chain':
                        op(0,'move',x=width//2,y=height//2);op(0,'key',key='ctrl+alt+shift+m');time.sleep(.8)
                        op(0,'move',x=width-1,y=height//2);wait_lease(0,1)
                        time.sleep(.65);op(0,'motion',dx=1600,dy=0);wait_lease(0,2,indices=(0,1,2))
                    else:
                        wait_lease(0,2,indices=(0,1,2))
                        recovered=[op(i,'status')['lease'] for i in range(3)]
                        check('Returning B learns the exact ongoing epoch without changing A/C control',all(l==lease_before_partition for l in recovered))
                        check('Resynchronizing B preserves the held modifier on C',op(2,'key_state',key='Shift_L'))
                        op(0,'key',action='keyup',key='Shift_L');time.sleep(.2)
                    check('C control resumes or persists after the relay reconnects',
                          all((op(i,'status').get('lease') or {}).get('focus')==nodes[2] for i in range(3)))
                    check('Relay recovery leaves no held modifier on C',not op(2,'key_state',key='Shift_L'))
                    op(0,'motion',dx=70,dy=0);time.sleep(.65)
                if args.target_restart:
                    for stop_op in ('stop','crash'):
                        label='Target C '+('SIGTERM restart' if stop_op=='stop' else 'SIGKILL restart')
                        op(0,'key',action='keydown',key='Shift_L')
                        op(0,'key',action='mousedown',key='1');time.sleep(.4)
                        check(label+': remote modifier and button are down and recorded',op(2,'key_state',key='Shift_L') and op(2,'button_state',button=1) and op(2,'injected_marker',node=nodes[2]))
                        op(2,stop_op,pid=agents.pop(2));time.sleep(5)
                        op(0,'key',action='keyup',key='Shift_L')
                        op(0,'key',action='mouseup',key='1')
                        check(label+': source native keyboard recovers locally',op(0,'local_key'))
                        # This other client's held key is absent from PoolSync's
                        # marker and must survive its recovery.
                        op(2,'key',action='keydown',key='Control_R')
                        op(2,'move',x=400,y=300);pointer_before=op(2,'pointer')
                        agents[2]=op(2,'start');time.sleep(4)
                        check(label+': old injected modifier/button are released',not op(2,'key_state',key='Shift_L') and not op(2,'button_state',button=1))
                        check(label+': recovery marker clears after acknowledged releases',not op(2,'injected_marker',node=nodes[2]))
                        check(label+': unrelated held key and local pointer are preserved',op(2,'key_state',key='Control_R') and op(2,'pointer')==pointer_before)
                        op(2,'key',action='keyup',key='Control_R')
                        check(label+': target native keyboard is usable',op(2,'local_key'))
                        op(0,'move',x=width//2,y=height//2);op(0,'key',key='ctrl+alt+shift+m');time.sleep(.8)
                        op(0,'move',x=width-1,y=height//2);wait_lease(0,1)
                        time.sleep(.65);op(0,'motion',dx=1600,dy=0);wait_lease(0,2,indices=(0,1,2))
                        op(0,'motion',dx=70,dy=0);time.sleep(.65)
                        op(2,'stop',pid=key_windows.pop(2));key_windows[2]=op(2,'key_window')
                        op(0,'key',key='v');time.sleep(.2)
                        check(label+': fresh remote input resumes in native GTK',op(2,'received_key').get('keyval')==ord('v'))
                        check(label+': normal key release leaves no recovery marker',not op(2,'injected_marker',node=nodes[2]))
                op(0,'motion',dx=-200,dy=0);wait_lease(0,1,indices=(0,1,2))
                check('A can return from C to B without changing controller')
                time.sleep(.65);op(0,'motion',dx=-70,dy=0);time.sleep(.2)
                op(0,'motion',dx=-1600,dy=0);wait_lease(0,0,indices=(0,1,2))
                check('A can return through B to its own native desktop',op(0,'local_key'))
                op(0,'move',x=width//2,y=height//2);time.sleep(.8)
                op(0,'move',x=width-1,y=height//2);wait_lease(0,1)
            op(0,'key',key='ctrl+alt+shift+m');wait_lease(0,0)
            check('Emergency shortcut returns captured input to A',op(0,'local_key'))
            check('Emergency return releases remote modifiers',all(not op(1,'key_state',key=k) for k in ('Shift_L','Control_L','Alt_L')))
            op(0,'move',x=width//2,y=height//2);time.sleep(.8);op(0,'move',x=width-1,y=height//2);wait_lease(0,1)
            op(0,'key',action='keydown',key='Shift_L');time.sleep(.4)
            check('A held modifier is injected on B',op(1,'key_state',key='Shift_L'))
            if args.network_loss:
                network_drop(0,True);time.sleep(5)
                check('Network loss releases the remote held modifier',not op(1,'key_state',key='Shift_L'))
                op(0,'key',action='keyup',key='Shift_L')
                check('Source native input recovers during a network partition',op(0,'local_key'))
                check('Target native input recovers during a network partition',op(1,'local_key'))
                network_drop(0,False);time.sleep(4)
                op(0,'move',x=width//2,y=height//2);op(0,'key',key='ctrl+alt+shift+m');time.sleep(.8)
                op(0,'move',x=width-1,y=height//2);wait_lease(0,1)
                check('Direct KVM resumes after the network partition')
                op(0,'key',action='keydown',key='Shift_L');time.sleep(.4)
            op(0,'stop',pid=agents.pop(0));time.sleep(5)
            check('Controller disappearance releases the held modifier on B',not op(1,'key_state',key='Shift_L'))
            check('B native keyboard is usable after the controller disappears',op(1,'local_key'))
            agents[0]=op(0,'start');time.sleep(4)
            op(1,'move',x=800,y=450);op(1,'key',key='ctrl+alt+shift+m');time.sleep(.5)
            op(1,'move',x=0,y=450);wait_lease(1,0)
            check('B takes control and crosses to A after A restarts')
            before=op(0,'pointer');b_width,b_height=op(1,'screen')
            for x in (b_width-30,30)*3:
                op(1,'move',x=x,y=b_height//2);time.sleep(.2)
            time.sleep(.65)
            lease=op(1,'status').get('lease',{})
            check('Reverse crossing remains on A despite repeated software warps on captured B',
                  op(0,'pointer')==before and lease.get('owner')==nodes[1] and lease.get('focus')==nodes[0])
            op(1,'motion',dx=-25,dy=10);time.sleep(.4)
            check('Reverse crossing still forwards relative device motion to A',op(0,'pointer')!=before)
            op(0,'away',value=True);time.sleep(5)
            check('A temporary departure returns B input locally',op(1,'status').get('lease') is None or op(1,'status')['lease']['focus']==nodes[1])
            check('B local GTK keyboard works after its remote target leaves',op(1,'local_key'))
            op(0,'away',value=False);time.sleep(2)
            if args.screen_changes:
                op(0,'screens',mode='dock');time.sleep(4)
                statuses=[op(i,'status') for i in range(3)]
                check('Two local monitors are announced directly',len(statuses[1]['peers'][nodes[0]]['monitors'])==2)
                check('Docking keeps the saved neighboring computer adjacent',all(s['topology']['nodes'][nodes[1]]['x']==3520 for s in statuses))
                op(0,'move',x=800,y=450);op(0,'key',key='ctrl+alt+shift+m');time.sleep(.8)
                op(0,'move',x=1599,y=450);time.sleep(.8)
                check('The internal monitor edge keeps input local',op(0,'status')['lease']['focus']==nodes[0])
                op(0,'move',x=2500,y=450);time.sleep(.4)
                check('The added local monitor remains directly usable',op(0,'pointer')['x']==2500)
                op(0,'move',x=3519,y=540);wait_lease(0,1)
                check('The external desktop edge reaches the other computer',op(1,'pointer')['x']<100)
                op(0,'key',action='keydown',key='Shift_L');time.sleep(.4)
                op(0,'screens',mode='mono');time.sleep(4);wait_lease(0,0)
                check('Unplugging a monitor during a grab releases remote modifiers',not op(1,'key_state',key='Shift_L'))
                op(0,'key',action='keyup',key='Shift_L')
                check('Unplugging a monitor during a grab returns native input locally',op(0,'local_key'))
                op(1,'screens',mode='small');time.sleep(4)
                check('Undocking restores the neighbor and advertises a different resolution',op(0,'status')['topology']['nodes'][nodes[1]]['x']==1600 and op(0,'status')['topology']['nodes'][nodes[1]]['width']==1366)
                op(0,'move',x=800,y=450);time.sleep(.8);op(0,'move',x=1599,y=450);wait_lease(0,1)
                pointer=op(1,'pointer')
                result['mixed_resolution_entry_pointer']=pointer
                check('Crossing scales coordinates between different resolutions',pointer['x']<100 and abs(pointer['y']-384)<=2)
                op(0,'key',key='ctrl+alt+shift+m');wait_lease(0,0)
                op(1,'screens',mode='mono');time.sleep(4)
                check('Original desktop dimensions return after screen restoration',op(0,'status')['topology']['nodes'][nodes[1]]['width']==1600)
            if args.simultaneous_claims:
                winners=[]
                for _ in range(10):
                    with ThreadPoolExecutor(max_workers=2) as executor:
                        futures=[executor.submit(op,i,'key',key='ctrl+alt+shift+m') for i in (0,1)]
                        for future in futures:future.result()
                    deadline=time.monotonic()+8
                    while time.monotonic()<deadline:
                        leases=[op(i,'status').get('lease') for i in range(3)]
                        if all(l and l==leases[0] for l in leases) and leases[0]['owner'] in nodes[:2] and leases[0]['focus']==leases[0]['owner']:break
                        time.sleep(.2)
                    else:raise AssertionError('Simultaneous claims did not converge')
                    winners.append(leases[0])
                result['simultaneous_claim_winners']=winners
                check('Ten concurrent claim rounds converge on one eligible controller')
                check('Concurrent claims leave both native keyboards available',op(0,'local_key') and op(1,'local_key'))
            if args.layout_ui:
                op(0,'ui_open');time.sleep(.4)
                op(0,'ui_click',x=210,y=16);time.sleep(.5)
                before=op(0,'saved_layout')
                op(0,'ui_drag',from_x=534,from_y=230,to_x=195,to_y=421);time.sleep(.5)
                op(0,'ui_click',x=355,y=62)
                deadline=time.monotonic()+8
                while time.monotonic()<deadline:
                    saved=[op(i,'saved_layout') for i in range(3)]
                    if all(d['revision']>before['revision'] and d['topology']['nodes'][nodes[1]]['x']==0 and d['topology']['nodes'][nodes[1]]['y']==900 for d in saved) and all(d==saved[0] for d in saved):break
                    time.sleep(.2)
                else:raise AssertionError('Native GTK drag/save did not persist the new layout on every peer')
                check('Native GTK layout edit persists and gossips without hub')
                result['native_layout_document']=saved[0]
                restart(0)
                deadline=time.monotonic()+8
                while time.monotonic()<deadline:
                    live=[op(i,'status').get('topology',{}).get('nodes',{}) for i in range(3)]
                    if all(d.get(nodes[1],{}).get('x')==0 and d.get(nodes[1],{}).get('y')==900 for d in live):break
                    time.sleep(.2)
                else:raise AssertionError('Saved layout did not survive an agent restart')
                check('Native saved layout survives restart and returns through direct presence')
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
            if args.soak_seconds:
                started=time.monotonic();rounds=0
                resource_samples.append([op(i,'resources') for i in range(3)])
                while time.monotonic()-started<args.soak_seconds:
                    i=rounds%3;image=('large-first' if (rounds//10)%2==0 else 'large-second') if args.large_images and rounds%10==0 else ('first' if rounds%2==0 else 'second')
                    since=time.time();op(i,'copy',image=image);expect((0,1,2),images[i][image],since)
                    copy_text((i+1)%3,'Soak image/text round '+str(rounds))
                    rounds+=1
                    if rounds%10==0:resource_samples.append([op(i,'resources') for i in range(3)])
                    time.sleep(.3)
                resource_samples.append([op(i,'resources') for i in range(3)])
                result['soak']={'elapsed_seconds':round(time.monotonic()-started,3),'image_text_rounds':rounds}
                check('Sustained real paste sequence finishes without stale contents')

            if args.idle_seconds:
                idle_start=[op(i,'resources') for i in range(3)];time.sleep(args.idle_seconds)
                idle_end=[op(i,'resources') for i in range(3)]
                result['idle_resources']=[{'elapsed_seconds':b['at']-a['at'],'cpu_percent':100*(b['cpu_seconds']-a['cpu_seconds'])/(b['at']-a['at']),'rss_start_kib':a['rss_kib'],'rss_end_kib':b['rss_kib']} for a,b in zip(idle_start,idle_end)]
                check('Idle resource interval finishes with all peers alive',all(a['pid']==b['pid'] for a,b in zip(idle_start,idle_end)))

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

            for wake_round in range(5):
                op(2,'away',value=True);time.sleep(.5)
                private_race=copy_text(2,'Private rejoin race '+str(wake_round),targets=(2,))
                with ThreadPoolExecutor(max_workers=2) as executor:
                    returning=executor.submit(op,2,'away',value=False)
                    office_copy=executor.submit(copy_text,0,'Concurrent office/rejoin '+str(wake_round),targets=(0,1))
                    returning.result();office_copy.result()
                copy_text(0,'Fresh office copy after concurrent rejoin '+str(wake_round))
                time.sleep(.5)
                check('Concurrent rejoin does not replay private clipboard round '+str(wake_round),all(op(i,'text')!=private_race for i in (0,1)))

            op(1, 'stop', pid=agents.pop(1));time.sleep(1)
            targets=(0,2) if args.links=='triangle' else (0,)
            isolated = copy_text(0, 'Copy while relay B offline', targets=targets);time.sleep(2)
            check('A local clipboard remains usable after the relay agent disappears', op(0, 'text') == isolated)
            if args.links=='triangle':check('An alternate direct route survives the loss of B',op(2,'text')==isolated)
            else:check('Disconnected chain C does not receive a new isolated copy',op(2,'text')!=isolated)
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
        if args.layout_ui:
            try:
                result['ui_failure_windows']=op(0,'ui_windows')
                result['ui_failure_snapshot']=op(0,'ui_snapshot')
            except Exception:pass
        result['error'] = str(error)
        print(json.dumps({'test_error': str(error)}), flush=True)
    finally:
        retained=args.retain_failed_desktops and not result.get('functional_checks_passed',False)
        result['failed_fresh_sessions_retained']=retained
        for _,pid,chain,rule in reversed(network_rules):
            try:subprocess.run(['nsenter','--target',pid,'--net','iptables','-D',chain,*rule],check=True,capture_output=True,timeout=5)
            except Exception as error:restoration['network-rule-cleanup-error']=str(error)
        for pid in (() if retained else browser_workers.values()):
            try:op(1,'stop',pid=pid)
            except Exception as error:restoration['browser-stop-error']=str(error)
        for processes in (() if retained else (receivers, key_windows, agents, proxies, buses, xservers)):
            for index, pid in list(processes.items()):
                try:op(index, 'stop', pid=pid)
                except Exception as error:restoration[str(index) + '-stop-error']=str(error)
        for index in ([] if retained else sorted(checked)):
            try:
                restoration[args.containers[index]]=op(index, 'restore', original=snapshots[index])
                if index in proxies:
                    restoration[args.containers[index]]['private_bandwidth_proxy_stopped']=not op(index,'process_alive',pid=proxies[index])
            except Exception as error:restoration[args.containers[index]]={'error':str(error)}
        result['restoration']=restoration
        result['restoration_passed']=len(restoration)==len(checked) and all(r.get('original_agent_restored') and r.get('original_pid_preserved') for r in restoration.values() if isinstance(r,dict))
        if args.peer_rate_mbit:result['restoration_passed']=result['restoration_passed'] and all(r.get('private_bandwidth_proxy_stopped') for r in restoration.values())
        result['checks_passed']=sum(check['passed'] for check in checks)
        result['test_root']=root
        args.output.write_text(json.dumps(result, indent=2) + '\n');args.output.chmod(0o600)
        print(json.dumps({'summary':result}), flush=True)
    raise SystemExit(0 if result.get('functional_checks_passed') and result['restoration_passed'] else 1)


if __name__ == '__main__':
    main()
