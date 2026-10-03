#!/usr/bin/env python3
"""Qualify native XRDP clipboard formats in an explicitly marked disposable container."""
import json,os,pathlib,subprocess,time,secrets,hashlib,signal,sys,base64,argparse
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--custom',action='store_true',help='Model the deployed image target list without running PoolSync')
parser.add_argument('--image-only',action='store_true',help='Remove empty text targets from the modeled offer')
parser.add_argument('--agent',action='store_true',help='Copy from an actual isolated encrypted peer; require POOLSYNC_EXPECTED_SHA256')
parser.add_argument('--pool-clipboard',action='store_true',help='DEV comparison: disable only native RDP clipboard and add a local PoolSync peer')
parser.add_argument('--browser',action='store_true',help='Check real Chromium Ctrl+V and three fresh image/text alternations')
args=parser.parse_args()
if args.pool_clipboard and not args.agent:parser.error('--pool-clipboard requires --agent')
if args.agent and args.custom:parser.error('--agent and --custom are separate qualification modes')
root=pathlib.Path('/opt/poolsync-rdp-lab')
if not pathlib.Path('/run/.containerenv').exists() or not (root/'.isolated-fixture').exists():
 raise SystemExit('Run only inside the explicitly provisioned disposable RDP fixture')
if '--image-only' in sys.argv and '--custom' not in sys.argv:
 raise SystemExit('--image-only requires --custom')
children=[]
def spawn(args,env=None,stdin=None,name='worker'):
 log=(root/(name+'.log')).open('ab');p=subprocess.Popen(args,env=env,stdin=stdin,stdout=log,stderr=log,start_new_session=True);children.append(p);return p
result={'image_only':'--image-only' in sys.argv,'scope':'isolated XRDP/FreeRDP '+('custom PNG/BMP offer' if '--custom' in sys.argv else 'native GTK baseline')+' without PoolSync','checks':[]}
try:
 subprocess.run(['useradd','-m','-s','/bin/bash','rdplab'],capture_output=True)
 password=secrets.token_urlsafe(24)
 subprocess.run(['chpasswd'],input=('rdplab:'+password+'\n').encode(),check=True,capture_output=True)
 home=pathlib.Path('/home/rdplab');(home/'session-env.json').unlink(missing_ok=True);(root/'pastes.json').unlink(missing_ok=True);uid=__import__('pwd').getpwnam('rdplab').pw_uid
 session=root/'session.py';session.write_text('import os,json,pathlib,gi\ngi.require_version("Gtk","3.0")\nfrom gi.repository import Gtk\np=pathlib.Path("/home/rdplab/session-env.json");p.write_text(json.dumps(dict(os.environ)));p.chmod(0o600)\nw=Gtk.Window(title="Isolated RDP fixture");w.show_all();Gtk.main()\n');session.chmod(0o755);root.chmod(0o755)
 p=home/'.xsession';p.write_text('#!/bin/sh\nexec dbus-run-session -- python3 /opt/poolsync-rdp-lab/session.py\n');p.chmod(0o755);os.chown(p,uid,uid)
 # Private disposable container only: no service or display on original desks.
 subprocess.run(['mkdir','-p','/run/xrdp'],check=True)
 spawn(['/usr/sbin/xrdp-sesman','--nodaemon'],name='sesman');spawn(['/usr/sbin/xrdp','--nodaemon'],name='xrdp')
 xv=spawn(['Xvfb',':98','-screen','0','1024x768x24','-nolisten','tcp'],name='client-xvfb');time.sleep(2)
 env=dict(os.environ,DISPLAY=':98');client=spawn(['/usr/bin/xfreerdp3','/v:127.0.0.1','/u:rdplab','/from-stdin','/cert:ignore','/sec:tls','-clipboard' if args.pool_clipboard else '+clipboard','/size:1024x768'],env=env,stdin=subprocess.PIPE,name='freerdp');client.stdin.write(('\n'+password+'\n').encode());client.stdin.close();del password
 until=time.monotonic()+30
 while not (home/'session-env.json').exists():
  assert client.poll() is None,'FreeRDP client exited; private logs retained'
  assert time.monotonic()<until,'RDP session startup timed out';time.sleep(.25)
 time.sleep(3)
 remote_env=json.loads((home/'session-env.json').read_text());result['server_display']=remote_env['DISPLAY'];result['checks'].append({'name':'Fresh authenticated native RDP session','passed':True})

 copy_env=remote_env;copy_user='rdplab'
 if '--agent' in sys.argv:
  binary=root/'candidate-agent';actual=hashlib.sha256(binary.read_bytes()).hexdigest()
  expected=os.environ.get('POOLSYNC_EXPECTED_SHA256')
  assert expected and actual==expected,'Exact candidate hash required'
  result['candidate_sha256']=actual
  result['scope']='Actual candidate direct encrypted mesh with native RDP clipboard '+('disabled' if args.pool_clipboard else 'enabled')
  result['native_rdp_clipboard_enabled']=not args.pool_clipboard
  source_env=dict(os.environ,DISPLAY=':99')
  spawn(['Xvfb',':99','-screen','0','1024x768x24','-nolisten','tcp'],name='source-xvfb');time.sleep(1)
  nodes={'rdp-source':(19701,'root',source_env),'rdp-remote':(19702,'rdplab',remote_env)}
  if args.pool_clipboard:nodes['rdp-client']=(19703,'root',env)
  tokens={node:secrets.token_hex(24) for node in nodes};e2e=base64.b64encode(secrets.token_bytes(32)).decode()
  agent_configs={}
  for node,(port,user,node_env) in nodes.items():
   d=root/(node+'-'+secrets.token_hex(6));d.mkdir(mode=0o700,exist_ok=True);who=__import__('pwd').getpwnam(user);os.chown(d,who.pw_uid,who.pw_gid)
   config=d/'agent.toml'
   lines=['node='+json.dumps(node),'hub_url="ws://127.0.0.1:1/ws"','token="unused-fixture-hub-token"','hubless=true','node_token='+json.dumps(tokens[node]),'e2e_key='+json.dumps(e2e),'mode="clipboard_only"','kvm_enabled=false','kvm_capture=false','peer_listen_port='+str(port),'peer_direct_clipboard=true','hub_clipboard=false','clipboard_poll_ms=80','pause_clipboard_when_rdp='+str(node=='rdp-client').lower(),'[screen]','width=1024','height=768','[peer_tokens]']
   lines += [other+'='+json.dumps(tokens[other]) for other in nodes if other!=node]
   for other,(other_port,_,_) in nodes.items():
    if other!=node:lines+=['[[neighbors]]','node='+json.dumps(other),'direction="right"','peer_url="ws://127.0.0.1:'+str(other_port)+'/ws"']
   config.write_text('\n'.join(lines)+'\n');config.chmod(0o600);os.chown(config,who.pw_uid,who.pw_gid)
   runtime=d/'runtime';runtime.mkdir(mode=0o700);os.chown(runtime,who.pw_uid,who.pw_gid)
   node_env=dict(node_env,XDG_RUNTIME_DIR=str(runtime),HOME=str(d),XDG_CONFIG_HOME=str(d/'config'),XDG_CACHE_HOME=str(d/'cache'),XDG_DATA_HOME=str(d/'data'))
   spawn(['/usr/sbin/runuser','-u',user,'--','env',*[k+'='+v for k,v in node_env.items()],'/usr/bin/dbus-run-session','--',str(binary),'--config',str(config)],name=node+'-agent')
   agent_configs[node]=config
  until=time.monotonic()+15
  while True:
   states=[]
   for c in agent_configs.values():
    status=c.with_suffix('.status.json')
    if status.exists():states.append(json.loads(status.read_text()))
   if len(states)==len(nodes) and all(len(v.get('peers',{}))==len(nodes) and all(p.get('active') for p in v['peers'].values()) for v in states):break
   assert time.monotonic()<until,'Actual isolated agents did not converge';time.sleep(.25)
  executing=[];agent_pids={}
  for proc in pathlib.Path('/proc').glob('[0-9]*'):
   try:
    proc_args=proc.joinpath('cmdline').read_bytes().split(b'\0')
    if proc_args[0]==str(binary).encode():
     owner=__import__('pwd').getpwuid(proc.stat().st_uid).pw_name
     q=subprocess.run(['/usr/sbin/runuser','-u',owner,'--','sha256sum',str(proc/'exe')],capture_output=True,text=True)
     assert q.returncode==0,'Executing hash unavailable to its own account'
     executing.append(q.stdout.split()[0])
     for node,config in agent_configs.items():
      if str(config).encode() in proc_args:agent_pids[node]=int(proc.name)
   except OSError:pass
  result['executing_sha256']=executing
  assert len(executing)==len(nodes) and all(h==actual for h in executing),'Actual executing candidate hashes differ'
  result['checks'].append({'name':'Actual candidate agents share active encrypted direct peers without hub','passed':True})
  result['peer_transport']='loopback WebSocket with configured XChaCha20-Poly1305 envelope; TLS is qualified separately'
  copy_env=source_env;copy_user='root'
 # Native GTK receiver requests image and text from the actual FreeRDP-owned selection.
 rec=spawn(['/usr/bin/python3',str(root/'clipboard-receiver.py'),'--watch','--quit-after','300','--log',str(root/'pastes.json')],env=env,name='receiver')
 text='Isolated RDP fresh text'
 textcode='import gi;gi.require_version("Gtk","3.0");from gi.repository import Gtk,Gdk;Gtk.init([]);Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).set_text("Isolated RDP fresh text",-1);Gtk.main()'
 copy=spawn(['/usr/sbin/runuser','-u',copy_user,'--','env',*[k+'='+v for k,v in copy_env.items()],'/usr/bin/python3','-c',textcode],name='remote-text-copy')
 text_hash=hashlib.sha256(text.encode()).hexdigest();until=time.monotonic()+10;textrecords=[]
 while time.monotonic()<until:
  if (root/'pastes.json').exists():
   textrecords=json.loads((root/'pastes.json').read_text())
   if any(x['kind']=='text' and x['sha256']==text_hash for x in textrecords):break
  time.sleep(.25)
 result['checks'].append({'name':'Fresh native GTK text across the selected clipboard route','passed':any(x['kind']=='text' and x['sha256']==text_hash for x in textrecords)})
 from PIL import Image
 image=Image.new('RGBA',(193,127),(37,199,81,255));path=root/'fresh.png';image.save(path);expected=hashlib.sha256(image.tobytes()).hexdigest()
 code='import gi,sys;gi.require_version("Gtk","3.0");from gi.repository import Gtk,Gdk,GdkPixbuf;Gtk.init([]);c=Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD);c.set_image(GdkPixbuf.Pixbuf.new_from_file(sys.argv[1]));Gtk.main()'
 copyargs=['/usr/bin/python3',str(root/'rdp-image-offer-owner.py'),str(path)] if '--custom' in sys.argv else ['/usr/bin/python3','-c',code,str(path)]
 if '--image-only' in sys.argv:copyargs.append('--image-only')
 copy=spawn(['/usr/sbin/runuser','-u',copy_user,'--','env',*[k+'='+v for k,v in copy_env.items()],*copyargs],name='remote-copy')
 until=time.monotonic()+20;records=[]
 while time.monotonic()<until:
  if (root/'pastes.json').exists():
   records=json.loads((root/'pastes.json').read_text())
   if any(x['kind']=='image' and x['sha256']==expected for x in records):break
  time.sleep(.25)
 result['targets']={}
 for side,ev in [('client',env),('server',remote_env)]:
  cmd=['timeout','4','xclip','-selection','clipboard','-o','-t','TARGETS']
  if side=='server':cmd=['/usr/sbin/runuser','-u','rdplab','--','env',*[k+'='+v for k,v in ev.items()],*cmd]
  q=subprocess.run(cmd,env=ev,capture_output=True,text=True);result['targets'][side]={'returncode':q.returncode,'targets':q.stdout.splitlines()}
 result['checks'].append({'name':'Fresh native GTK image pasted with matching pixels across the selected route','passed':any(x['kind']=='image' and x['sha256']==expected for x in records),'expected_sha256':expected,'observed_records':records})

 if '--browser' in sys.argv:
  browser_log=root/'browser-pastes.json';browser_log.unlink(missing_ok=True)
  spawn(['/usr/bin/python3',str(root/'browser-paste-server.py'),'--html',str(root/'clipboard-paste.html'),'--log',str(browser_log),'--port','19582'],name='browser-server')
  profile=root/('browser-profile-'+secrets.token_hex(6))
  spawn(['/usr/bin/chromium','--no-sandbox','--disable-dev-shm-usage','--ozone-platform=x11','--no-proxy-server','--password-store=basic','--disable-background-networking','--no-first-run','--no-default-browser-check','--user-data-dir='+str(profile),'--new-window','http://127.0.0.1:19582/'],env=env,name='browser')
  until=time.monotonic()+20
  while True:
   q=subprocess.run(['xdotool','search','--name','PoolSync clipboard paste test'],env=env,capture_output=True,text=True)
   if q.returncode==0 and q.stdout.split():window=q.stdout.split()[-1];break
   assert time.monotonic()<until,'Native Chromium window unavailable';time.sleep(.2)
  time.sleep(1)
  def browser_paste(expected_hash):
   subprocess.run(['xdotool','windowraise',window,'windowfocus','--sync',window,'key','--clearmodifiers','ctrl+v'],env=env,check=True,capture_output=True)
   until=time.monotonic()+10
   while time.monotonic()<until:
    if browser_log.exists():
     entries=json.loads(browser_log.read_text())
     if any(e['sha256']==expected_hash for e in entries):return entries
    time.sleep(.2)
   raise AssertionError('Native RDP browser paste hash did not converge')
  entries=browser_paste(expected)
  result['checks'].append({'name':'Actual native Chromium Ctrl+V pastes the image across the selected route','passed':True})
  for index in range(3):
   value='RDP alternating synthetic text '+str(index)+' '+secrets.token_hex(8)
   textcode='import gi,sys;gi.require_version("Gtk","3.0");from gi.repository import Gtk,Gdk;Gtk.init([]);Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).set_text(sys.argv[1],-1);Gtk.main()'
   spawn(['/usr/sbin/runuser','-u',copy_user,'--','env',*[k+'='+v for k,v in copy_env.items()],'/usr/bin/python3','-c',textcode,value],name='alternating-text')
   expected_hash=hashlib.sha256(value.encode()).hexdigest();until=time.monotonic()+15
   while True:
    entries=json.loads((root/'pastes.json').read_text())
    if any(e['sha256']==expected_hash for e in entries):break
    assert time.monotonic()<until,'Native RDP text did not converge';time.sleep(.2)
   browser_paste(expected_hash)
   new_image=Image.new('RGBA',(83+index,59+index),(51+index*30,91+index*20,191-index*40,255));image_path=root/('alternating-'+str(index)+'.png');new_image.save(image_path)
   spawn(['/usr/sbin/runuser','-u',copy_user,'--','env',*[k+'='+v for k,v in copy_env.items()],'/usr/bin/python3','-c',code,str(image_path)],name='alternating-image')
   expected_hash=hashlib.sha256(new_image.tobytes()).hexdigest();until=time.monotonic()+15
   while True:
    entries=json.loads((root/'pastes.json').read_text())
    if any(e['sha256']==expected_hash for e in entries):break
    assert time.monotonic()<until,'Native RDP image did not converge';time.sleep(.2)
   browser_paste(expected_hash)
   result['checks'].append({'name':'Actual RDP GTK and Chromium image/text alternation '+str(index),'passed':True})

  if args.pool_clipboard:
   os.kill(agent_pids['rdp-remote'],signal.SIGTERM);time.sleep(2)
   value='Pool clipboard stays available without the RDP destination agent '+secrets.token_hex(8)
   spawn(['/usr/sbin/runuser','-u',copy_user,'--','env',*[k+'='+v for k,v in copy_env.items()],'/usr/bin/python3','-c',textcode,value],name='destination-loss-copy')
   expected_hash=hashlib.sha256(value.encode()).hexdigest();until=time.monotonic()+15
   while True:
    entries=json.loads((root/'pastes.json').read_text())
    if any(e['sha256']==expected_hash for e in entries):break
    assert time.monotonic()<until,'Local pool clipboard depends on the destination agent';time.sleep(.2)
   browser_paste(expected_hash)
   result['checks'].append({'name':'Local GTK and Chromium clipboard remain usable after the RDP destination PoolSync agent exits','passed':True})
  result['native_browser_records']=json.loads(browser_log.read_text())
 result['passed']=all(x['passed'] for x in result['checks'])
except Exception as e:result.update(passed=False,error=str(e))
finally:
 for p in reversed(children):
  if p.poll() is None:
   try:os.killpg(p.pid,signal.SIGTERM)
   except ProcessLookupError:pass
 for p in children:
  try:p.wait(timeout=3)
  except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait()
 if (root/'browser-pastes.json').exists():
  try:result['native_browser_records']=json.loads((root/'browser-pastes.json').read_text())
  except (OSError,ValueError):pass
 (root/('baseline-result-'+str(time.time_ns())+'.json')).write_text(json.dumps(result,indent=2)+'\n');(root/'baseline-result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))

# A retained failure must also fail shell-driven qualification.
sys.exit(0 if result.get("passed") else 1)
