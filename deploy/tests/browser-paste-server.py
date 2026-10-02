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
const nativeImageReceiver=receiveImage;
receiveImage=async(blob,route)=>{
  await nativeImageReceiver(blob,route);
  const bitmap=await createImageBitmap(blob),canvas=document.createElement('canvas');
  canvas.width=bitmap.width;canvas.height=bitmap.height;
  const ctx=canvas.getContext('2d');ctx.drawImage(bitmap,0,0);
  const sha256=await digest(ctx.getImageData(0,0,canvas.width,canvas.height).data);
  await fetch('/record',{method:'POST',body:JSON.stringify({kind:'image',route,sha256,width:bitmap.width,height:bitmap.height})});
  bitmap.close();
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
            if self.path != '/record' or not 0 < int(self.headers.get('Content-Length', 0)) < 4096:
                self.send_error(400)
                return
            record = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
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
