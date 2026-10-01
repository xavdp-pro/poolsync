#!/usr/bin/env python3
"""Exercise a disposable hub through its HTTP/WebSocket protocol.

Start an isolated hub, then pass its URL and set POOLSYNC_TEST_TOKEN.
No installed desktop configuration is read or changed.
"""
import base64, json, os, socket, struct, sys, time, urllib.request

base = sys.argv[1].rstrip('/')
token = os.environ['POOLSYNC_TEST_TOKEN']
url = urllib.parse.urlparse(base)
assert url.scheme == 'http', 'Use a disposable local test hub'

def api(path, body=None):
    request = urllib.request.Request(base + path, data=None if body is None else json.dumps(body).encode(), headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'})
    with urllib.request.urlopen(request, timeout=4) as response:
        raw = response.read()
        return json.loads(raw) if raw else None

class Client:
    def __init__(self, name):
        self.name = name
        self.socket = socket.create_connection((url.hostname, url.port), timeout=4)
        key = base64.b64encode(os.urandom(16)).decode()
        self.socket.sendall((f'GET /ws HTTP/1.1\r\nHost: {url.hostname}:{url.port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\nAuthorization: Bearer {token}\r\nx-poolsync-node: {name}\r\n\r\n').encode())
        response = b''
        while b'\r\n\r\n' not in response:
            response += self.socket.recv(1)
        assert b'101 Switching Protocols' in response, response.split(b'\r\n')[0]
    def send(self, message):
        data = json.dumps(message).encode();mask = os.urandom(4)
        header = b'\x81' + (bytes([128+len(data)]) if len(data)<126 else b'\xfe'+struct.pack('!H',len(data)))
        self.socket.sendall(header + mask + bytes(b ^ mask[i%4] for i,b in enumerate(data)))
    def hello(self, width, active=True, mode='full', monitors=None):
        self.send({'type':'hello','node':self.name,'mode':mode,'screen':{'width':width,'height':768},'neighbors':[],'kvm_enabled':active and mode=='full','local_active':active,'clipboard_sync':True,'monitors':monitors or []})
    def close(self):
        self.socket.close()

def wait_for(check):
    end = time.monotonic()+5
    while time.monotonic()<end:
        if check(): return
        time.sleep(.1)
    raise AssertionError('Hub state did not converge')

clients=[]
try:
    a,b,c = [Client('test-daily-'+name) for name in ['a','b','clip']];clients=[a,b,c]
    a.hello(1366);b.hello(1920);c.hello(800,mode='clipboard_only')
    wait_for(lambda: all(name in api('/api/topology')['nodes'] for name in [a.name,b.name,c.name]))
    topology=api('/api/topology');topology['nodes'][a.name].update(x=-1366,y=-200);topology['nodes'][b.name].update(x=0,y=0);api('/api/topology',topology)
    for client in [a,b]:
        client.send({'type':'master_claim','node':client.name,'ts':0});wait_for(lambda:api('/api/status')['master']==client.name)
    c.send({'type':'master_claim','node':c.name,'ts':0});time.sleep(.2);assert api('/api/status')['master']==b.name
    b.hello(2560,monitors=[{'name':'primary','x':0,'y':0,'width':2560,'height':1440,'primary':True},{'name':'extra','x':2560,'y':0,'width':1920,'height':1080}])
    wait_for(lambda:api('/api/topology')['nodes'][b.name]['width']==2560)
    wait_for(lambda:len(next(n for n in api('/api/status')['nodes'] if n['name']==b.name)['monitors'])==2)
    b.hello(2560,active=False);wait_for(lambda:api('/api/status')['master'] is None)
    assert api('/api/topology')['nodes'][b.name]['x']==0
    b.hello(2560);b.send({'type':'master_claim','node':b.name,'ts':0});wait_for(lambda:api('/api/status')['master']==b.name)
    b.close();wait_for(lambda:api('/api/status')['master'] is None)
    assert api('/api/topology')['nodes'][b.name]['x']==0
    b=Client(b.name);clients.append(b);b.hello(2560);wait_for(lambda:next(n for n in api('/api/status')['nodes'] if n['name']==b.name)['online'])
    print(json.dumps({'passed':True,'checks':['dynamic master','clipboard-only exclusion','mixed resolutions','negative positions','monitor announcement','temporary absence','master disconnect','position-preserving rejoin']}))
finally:
    for client in clients:client.close()
