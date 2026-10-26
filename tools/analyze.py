#!/usr/bin/env python3
"""Explicit, one-shot server-side provider adapter. Dry-run unless --send is set.

Meta Model API supports image review and advisory security telemetry.
The optional generic adapter uses POST JSON {device, jpeg_base64}.
No automatic retries. Provider-specific payload adaptation belongs here, never
on the camera. Not a built-in connection to any paid service.
"""
import argparse
import base64
import json
import hashlib
import os
import pathlib
import socket
import signal
import time
from review_store import reserve_budget, save_report
import urllib.error
import urllib.parse
import urllib.request
from security_review import strict_json, telemetry, policy
import meta_provider

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
            return strict_json(data)
    except (TimeoutError,socket.timeout,urllib.error.URLError,OSError,ValueError) as e:
        raise UnknownOutcome('Provider outcome unknown or rejected; no retry was made. Check provider records before retrying.') from e
    finally:
        signal.alarm(0)
        signal.signal(signal.SIGALRM, previous)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--device',default='demo'); p.add_argument('--config',default='runtime/config.secret')
    p.add_argument('--provider',choices=['meta','generic'],default='meta')
    p.add_argument('--mode',choices=['image','security','incident'],default='image')
    p.add_argument('--model',help='Explicit Meta model ID from your authorized API documentation')
    p.add_argument('--daily-budget',type=int,default=10)
    p.add_argument('--cooldown',type=int,default=60)
    p.add_argument('--report-dir',help='Existing server data root; store one bounded local report per device')
    group=p.add_mutually_exclusive_group();group.add_argument('--send',action='store_true');group.add_argument('--mock',action='store_true',help='Synthetic incident response; no external call')
    args=p.parse_args()
    if args.mock and (args.mode!='incident' or args.provider!='meta'): p.error('--mock is only for Meta incident demo')
    if args.report_dir and args.mode!='incident': p.error('Reports require incident mode')
    if not all(c in 'abcdefghijklmnopqrstuvwxyz0123456789-' for c in args.device) or not 1<=len(args.device)<=24: p.error('Invalid device ID')
    if args.provider=='generic' and args.mode!='image': p.error('Generic adapter supports the image contract only')
    if args.send and args.provider=='meta' and not args.model: p.error('--model is required for a real Meta request')
    cfg_path=pathlib.Path(args.config)
    if cfg_path.stat().st_mode & 0o077: p.error('Config must be mode 600')
    cfg=dict(line.split('=',1) for line in cfg_path.read_text().splitlines() if line and not line.startswith('#'))
    image_sequence=None
    sampled=int(time.time())
    def local(path):
        nonlocal image_sequence
        req=urllib.request.Request('http://127.0.0.1:8080'+path,headers={'Authorization':'Bearer '+cfg['viewer']})
        with urllib.request.build_opener(NoRedirect).open(req,timeout=5) as r:
            if path.startswith('/api/latest/'):
                image_sequence=int(r.headers['X-Sequence'])
                if not 0<image_sequence<=2**64-1: raise ValueError('Invalid evidence sequence')
            return r.read(MAX_FRAME+1)
    clean=telemetry(strict_json(local('/api/security'))) if args.mode in ('security','incident') else None
    data=local('/api/latest/'+args.device) if args.mode in ('image','incident') else None
    if args.provider=='meta':
        body=meta_provider.request_body(args.mode,args.model or 'preview-model-not-selected',jpeg=data,clean=clean)
    else: body=payload(args.device,data)
    if not args.send and not args.mock:
        result={'mode':'dry-run','provider':args.provider,'task':args.mode,'request_bytes':len(body),'external_requests':0}
        if clean is not None: result['security_review']=policy(clean)
        else: result['jpeg_bytes']=len(data)
        print(json.dumps(result)); return
    # Only local operator settings choose providers. Meta's endpoint is fixed to official docs.
    url=meta_provider.ENDPOINT if args.provider=='meta' else endpoint(os.environ['XLENS_PROVIDER_URL'])
    key=private_secret(os.environ['XLENS_META_KEY_FILE' if args.provider=='meta' else 'XLENS_PROVIDER_KEY_FILE']) if args.send else None
    import fcntl
    lock=cfg_path.parent/'analysis.lock'
    with open(lock,'w') as f:
        try: fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError: raise SystemExit('Another analysis is running; no request sent')
        if args.send:
            reserve_budget(cfg_path.parent,args.daily_budget,args.cooldown)
            result=call_provider(url,key,body)
        else:
            # Clearly labeled fixture exercises the real response validator and report pipeline.
            evidence=policy(clean)['evidence']
            value={'visual':{'objects':['other'],'summary':'Synthetic X-Lens test pattern, not real camera footage.','uncertain':True},
                   'security':{'risk':'medium' if evidence!=['no_signal'] else 'low','recommendation':'review','reason_codes':evidence},
                   'incident_summary':'SIMULATED AI: a synthetic visual signal is paired with observed receiver counters. Replays may indicate counter recovery or abuse; inspect the device before changing access. No intrusion is established.'}
            result={'choices':[{'finish_reason':'stop','message':{'content':json.dumps(value)}}]}

        if args.provider=='meta':
            try: result=meta_provider.response_value(result,args.mode)
            except ValueError as e: raise UnknownOutcome('Meta output rejected; provider may have completed/billed the call. No retry.') from e
        if args.mode=='incident':
            result={'version':1,'device':args.device,'created_unix':int(time.time()),'source':'meta' if args.send else 'mock',
                    'model':args.model if args.send else 'synthetic-fixture','assessment':result,
                    'policy':policy(clean,result['security']),'actions_executed':False,
                    'evidence':{'sequence':image_sequence,'image_sha256':hashlib.sha256(data).hexdigest(),
                                'jpeg_bytes':len(data),'sampled_unix':sampled,'telemetry':clean}}
            if args.report_dir: save_report(args.report_dir,args.device,result)
        elif clean is not None: result=policy(clean,result)
    print(json.dumps(result,ensure_ascii=True))
if __name__=='__main__':
    try: main()
    except (ValueError,UnknownOutcome,KeyError,urllib.error.URLError) as e: raise SystemExit(str(e))
