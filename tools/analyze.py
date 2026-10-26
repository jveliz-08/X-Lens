#!/usr/bin/env python3
"""Explicit, one-shot server-side provider adapter. Dry-run unless --send is set.

Provider contract: POST JSON {device, jpeg_base64}; JSON response <=64 KiB.
No automatic retries. Provider-specific payload adaptation belongs here, never
on the camera. Not a built-in connection to any paid service.
"""
import argparse
import base64
import json
import os
import pathlib
import socket
import signal
import urllib.error
import urllib.parse
import urllib.request

MAX_FRAME=131072
MAX_RESPONSE=65536
class UnknownOutcome(Exception): pass
class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,*args,**kwargs): return None

def endpoint(url):
    p=urllib.parse.urlsplit(url)
    if p.scheme!='https' or not p.hostname or p.username or p.password or p.fragment or p.query:
        raise ValueError('Provider URL must be an administrator-selected HTTPS endpoint without credentials, query, or fragment')
    return url

def private_secret(path):
    p=pathlib.Path(path)
    if p.stat().st_mode & 0o077: raise ValueError('Secret file must be mode 600')
    value=p.read_text().strip()
    if not value or len(value)>4096 or any(ord(c)<33 or ord(c)>126 for c in value): raise ValueError('Invalid secret')
    return value

def payload(device,jpeg):
    if not 4<=len(jpeg)<=MAX_FRAME or not jpeg.startswith(b'\xff\xd8') or not jpeg.endswith(b'\xff\xd9'):
        raise ValueError('Invalid or oversized JPEG')
    return json.dumps({'device':device,'jpeg_base64':base64.b64encode(jpeg).decode()},separators=(',',':')).encode()

def call_provider(url,key,body,opener=None):
    endpoint(url)
    request=urllib.request.Request(url,body,{'Authorization':'Bearer '+key,'Content-Type':'application/json'},method='POST')
    opener=opener or urllib.request.build_opener(NoRedirect)
    def deadline(signum, frame):
        raise TimeoutError('provider absolute deadline')
    previous = signal.signal(signal.SIGALRM, deadline)
    signal.alarm(20)
    try:
        with opener.open(request,timeout=15) as r:
            data=r.read(MAX_RESPONSE+1)
            if len(data)>MAX_RESPONSE: raise UnknownOutcome('Provider response too large; request may have completed. Do not retry automatically.')
            return json.loads(data)
    except (TimeoutError,socket.timeout,urllib.error.URLError,OSError,ValueError) as e:
        raise UnknownOutcome('Provider outcome unknown or rejected; no retry was made. Check provider records before retrying.') from e
    finally:
        signal.alarm(0)
        signal.signal(signal.SIGALRM, previous)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--device',default='demo'); p.add_argument('--config',default='runtime/config.secret')
    p.add_argument('--send',action='store_true'); args=p.parse_args()
    if not all(c in 'abcdefghijklmnopqrstuvwxyz0123456789-' for c in args.device) or not 1<=len(args.device)<=24: p.error('Invalid device ID')
    cfg_path=pathlib.Path(args.config)
    if cfg_path.stat().st_mode & 0o077: p.error('Config must be mode 600')
    cfg=dict(line.split('=',1) for line in cfg_path.read_text().splitlines() if line and not line.startswith('#'))
    req=urllib.request.Request('http://127.0.0.1:8080/api/latest/'+args.device,headers={'Authorization':'Bearer '+cfg['viewer']})
    with urllib.request.build_opener(NoRedirect).open(req,timeout=5) as r: image=r.read(MAX_FRAME+1)
    body=payload(args.device,image)
    if not args.send: print(json.dumps({'mode':'dry-run','device':args.device,'jpeg_bytes':len(image),'request_bytes':len(body),'external_requests':0})); return
    # Operator-only environment settings; never accept a provider URL from HTTP/device input.
    url=endpoint(os.environ['XLENS_PROVIDER_URL']); key=private_secret(os.environ['XLENS_PROVIDER_KEY_FILE'])
    # POSIX lock bounds concurrent paid requests for this host, independent of invocation count.
    import fcntl
    lock=cfg_path.parent/'analysis.lock'
    with open(lock,'w') as f:
        try: fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError: raise SystemExit('Another analysis is running; no request sent')
        result=call_provider(url,key,body)
    # JSON escapes provider control characters. Do not render provider output as HTML.
    print(json.dumps(result,ensure_ascii=True))
if __name__=='__main__':
    try: main()
    except (ValueError,UnknownOutcome,KeyError,urllib.error.URLError) as e: raise SystemExit(str(e))
