#!/usr/bin/env python3
"""Observe only selection protocol metadata on an explicitly isolated X11 lab.

Never log property bytes, text, key or mouse events. End after a bounded interval.
The existing display/clients are not proxied or reconfigured.
"""
import argparse
import json
import os
import struct
import threading
import time
from pathlib import Path
from Xlib import display
from Xlib.error import ConnectionClosedError
from Xlib.ext import record

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--root',type=Path,required=True)
parser.add_argument('--seconds',type=int,default=180)
args=parser.parse_args()
assert 1<=args.seconds<=1200
assert str(args.root).startswith('/tmp/poolsync-no-hub-')
environment=json.loads((args.root/'environment.json').read_text())
assert environment['DISPLAY']==':110', 'Restricted to the isolated lab display'
os.environ.update(DISPLAY=environment['DISPLAY'],XAUTHORITY=environment['XAUTHORITY'])
control=display.Display();data=display.Display()
atoms={name:control.intern_atom(name) for name in ('GDK_SELECTION','image/png','image/bmp','INCR','CLIPBOARD')}
assert data.has_extension('RECORD')
context=data.record_create_context(0,[record.AllClients],[{
    'core_requests':(19,24),'core_replies':(0,0),
    'ext_requests':(0,0,0,0),'ext_replies':(0,0,0,0),
    'delivered_events':(28,31),'device_events':(0,0),'errors':(0,0),
    'client_started':False,'client_died':False}])
def stop():
    try:
        control.record_disable_context(context);control.flush()
    except ConnectionClosedError:
        pass
timer=threading.Timer(args.seconds,stop);timer.daemon=True;timer.start()
count=0
log_path=args.root/'selection-protocol.jsonl'
with log_path.open('w',buffering=1) as output:
    log_path.chmod(0o600)
    def log(reply, **fields):
        global count
        count+=1
        if count<=50000:
            output.write(json.dumps({'at':time.time(),'client':reply.id_base,**fields})+'\n')
    def receive(reply):
        if reply.category not in (record.FromClient,record.FromServer) or reply.client_swapped:
            return
        payload=reply.data;offset=0
        while offset+4<=len(payload):
            operation=payload[offset];head=payload[offset:offset+32]
            if reply.category==record.FromServer:
                if len(head)<32:return
                size=32
                if operation==28 and struct.unpack_from('<I',head,8)[0]==atoms['GDK_SELECTION']:
                    log(reply,event='PropertyNotify',window=struct.unpack_from('<I',head,4)[0],server_time=struct.unpack_from('<I',head,12)[0],state=head[16])
                elif operation&127==31 and struct.unpack_from('<I',head,12)[0]==atoms['CLIPBOARD']:
                    log(reply,event='SelectionNotify',window=struct.unpack_from('<I',head,8)[0],target=struct.unpack_from('<I',head,16)[0],property=struct.unpack_from('<I',head,20)[0],server_time=struct.unpack_from('<I',head,4)[0])
            else:
                words=struct.unpack_from('<H',head,2)[0];shift=0
                if not words:
                    words=struct.unpack_from('<I',head,4)[0];shift=4
                size=words*4
                if size<=0:return
                if operation in (19,20) and len(head)>=12+shift:
                    window,prop=struct.unpack_from('<II',head,4+shift)
                    if prop==atoms['GDK_SELECTION']:
                        fields={'event':{19:'DeleteProperty',20:'GetProperty'}[operation],'window':window}
                        log(reply,**fields)
                elif operation==24 and len(head)>=24+shift and struct.unpack_from('<I',head,8+shift)[0]==atoms['CLIPBOARD']:
                    log(reply,event='ConvertSelection',window=struct.unpack_from('<I',head,4+shift)[0],target=struct.unpack_from('<I',head,12+shift)[0],property=struct.unpack_from('<I',head,16+shift)[0],server_time=struct.unpack_from('<I',head,20+shift)[0])
            offset+=size
    try:
        data.record_enable_context(context,receive)
    except ConnectionClosedError:
        pass
    finally:
        timer.cancel()
        for connection in (data,control):
            try:connection.close()
            except ConnectionClosedError:pass
