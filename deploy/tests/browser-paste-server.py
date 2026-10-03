#!/usr/bin/env python3
"""Loopback-only native browser paste proof. Store metadata, never clipboard data.

Serve the existing manual receiver with an additional lab-only reporting hook.
The hook runs its real paste handler and hashes the browser's received pixels.
Ctrl+V must be generated in the native browser; no virtual clipboard API is used.
"""

import argparse
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
from pathlib import Path
import re
import time


HOOK = '''<script>
const tracePaste=(event,extra={})=>fetch('/diagnostic',{method:'POST',body:JSON.stringify({event,
  paste_target_focused:document.activeElement===$('pasteTarget'),document_focused:document.hasFocus(),...extra})}).catch(()=>{});
window.addEventListener('focus',()=>{$('pasteTarget').focus();tracePaste('focus');});
window.addEventListener('keydown',event=>{if(event.ctrlKey&&event.code==='KeyV')tracePaste('paste-shortcut');},true);
document.addEventListener('paste',event=>{const images=Array.from(event.clipboardData.files).filter(file=>file.type.startsWith('image/'));
  tracePaste('paste-dispatch',{image_files:images.length,first_image_bytes:images[0]?.size||0});},true);
const nativeImageReceiver=receiveImage;
receiveImage=async(blob,route)=>{
  let rendered=false;
  try {
  await nativeImageReceiver(blob,route);
  rendered=true;
  const bitmap=await createImageBitmap(blob),canvas=document.createElement('canvas');
  canvas.width=bitmap.width;canvas.height=bitmap.height;
  const ctx=canvas.getContext('2d');ctx.drawImage(bitmap,0,0);
  const sha256=await digest(ctx.getImageData(0,0,canvas.width,canvas.height).data);
  await fetch('/record',{method:'POST',body:JSON.stringify({kind:'image',route,sha256,width:bitmap.width,height:bitmap.height})});
  bitmap.close();
  } catch(error) {tracePaste('image-error',{error_name:error.name,rendered});throw error;}
};
const nativeTextReceiver=receiveText;
receiveText=async(text,route)=>{
  nativeTextReceiver(text,route);
  const sha256=await digest(new TextEncoder().encode(text));
  await fetch('/record',{method:'POST',body:JSON.stringify({kind:'text',route,sha256,length:text.length})});
};
setTimeout(()=>$('pasteTarget').focus(),200);
</script>'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--html', type=Path, required=True)
    parser.add_argument('--log', type=Path, required=True)
    parser.add_argument('--port', type=int, default=19580)
    args = parser.parse_args()
    page = args.html.read_text().replace('</html>', HOOK + '</html>').encode()
    records = []
    diagnostics = []
    diagnostic_path=args.log.with_name(args.log.stem+'-diagnostics.json')
    diagnostic_path.write_text('[]\n');diagnostic_path.chmod(0o600)
    args.log.write_text('[]\n')
    args.log.chmod(0o600)

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            if self.path not in ('/', '/clipboard-paste.html'):
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Cache-Control', 'no-store')
            self.end_headers()
            self.wfile.write(page)

        def do_POST(self):
            if self.path not in ('/record','/diagnostic') or not 0 < int(self.headers.get('Content-Length', 0)) < 4096:
                self.send_error(400)
                return
            record = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            if self.path=='/diagnostic':
                if record.get('event') not in ('focus','paste-shortcut','paste-dispatch','image-error') or any(type(record.get(k)) is not bool for k in ('paste_target_focused','document_focused')):
                    self.send_error(400);return
                fields=('event','paste_target_focused','document_focused')
                if record['event']=='paste-dispatch':
                    if any(type(record.get(k)) is not int or not 0<=record[k]<=64*1024*1024 for k in ('image_files','first_image_bytes')):
                        self.send_error(400);return
                    fields+=('image_files','first_image_bytes')
                if record['event']=='image-error':
                    if not isinstance(record.get('error_name'),str) or not re.fullmatch('[A-Za-z0-9_.-]{1,64}',record['error_name']) or type(record.get('rendered')) is not bool:
                        self.send_error(400);return
                    fields+=('error_name','rendered')
                diagnostics.append({'at':time.time(),**{k:record[k] for k in fields}})
                del diagnostics[:-500]
                diagnostic_path.write_text(json.dumps(diagnostics)+'\n')
                self.send_response(204);self.end_headers();return
            if (record.get('kind') not in ('text', 'image')
                    or record.get('route') != 'Ctrl+V paste event'
                    or not re.fullmatch('[0-9a-f]{64}', record.get('sha256', ''))):
                self.send_error(400)
                return
            allowed = ('kind', 'route', 'sha256', 'width', 'height', 'length')
            records.append({'at': time.time(), **{k: record[k] for k in allowed if k in record}})
            args.log.write_text(json.dumps(records) + '\n')
            self.send_response(204)
            self.end_headers()

    HTTPServer(('127.0.0.1', args.port), Handler).serve_forever()


if __name__ == '__main__':
    main()
