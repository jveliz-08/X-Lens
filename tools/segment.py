#!/usr/bin/env python3
"""SAM 3.1 ignore-zone proposal. Invoked only by the server:

    python3 tools/segment.py <meta|mock> <private state dir>   (JSON {concept, frame} on stdin)

Finds every instance of a short concept ("tree", "television") in the camera's latest
frame with Meta SAM 3.1 (Responses API, text prompt) and prints the 8x6 motion grid cells
the boxes cover. It only proposes: the operator reviews the grid and saves it separately.
One budgeted call, never retried. Mock mode makes no call and proposes nothing.
"""
import base64
import fcntl
import json
import os
import pathlib
import re
import sys
from analyze import private_secret
from meta_media import RESPONSES, post
from review_store import reserve_slot
from security_review import strict_json
from vision import DAILY, MIN_GAP, clean

COLS,ROWS=8,6
MAX_BOXES=16
BOX=re.compile(r'<\|box;([^|>]{1,200})\|>')

def body(concept,jpeg):
    return json.dumps({'model':'sam-3.1','stream':True,'metadata':{'mask_encoding':'one_bit'},
        'input':[{'type':'message','role':'user','content':[{'type':'input_text','text':concept},
            {'type':'input_image','image_url':'data:image/jpeg;base64,'+base64.b64encode(jpeg).decode()}]}]},separators=(',',':')).encode()

def stream_text(raw):
    """Concatenate response.output_text.delta events from a Responses API event stream."""
    out=[]
    for line in raw.decode('utf-8').splitlines():
        if not line.startswith('data:'): continue
        data=line[5:].strip()
        if not data or data=='[DONE]': continue
        event=strict_json(data)
        if not isinstance(event,dict): raise ValueError('Invalid stream event')
        if event.get('type') in ('error','response.failed'): raise ValueError('Provider error')
        if event.get('type')=='response.output_text.delta':
            if not isinstance(event.get('delta'),str): raise ValueError('Invalid delta')
            out.append(event['delta'])
    return ''.join(out)

def boxes(text):
    """Pixel boxes from the SAM text lane: <|box;x1=..;y1=..;x2=..;y2=..;w=..;h=..[;c=..]|>."""
    found=[]
    for m in BOX.finditer(text):
        f=dict(p.split('=',1) for p in m.group(1).split(';') if '=' in p)
        try: x1,y1,x2,y2,w,h=(float(f[k]) for k in ('x1','y1','x2','y2','w','h'))
        except (KeyError,ValueError) as e: raise ValueError('Invalid box') from e
        if not (0<w<=4096 and 0<h<=4096 and 0<=x1<x2<=w and 0<=y1<y2<=h): raise ValueError('Box out of range')
        found.append((x1,y1,x2,y2,w,h))
        if len(found)>=MAX_BOXES: break
    return found

def mask(found,cover=0.3):
    """Grid cells (bit row*8+col) at least `cover` covered by any box."""
    bits=0
    for x1,y1,x2,y2,w,h in found:
        cw,ch=w/COLS,h/ROWS
        for r in range(ROWS):
            for c in range(COLS):
                ox=max(0,min(x2,(c+1)*cw)-max(x1,c*cw)); oy=max(0,min(y2,(r+1)*ch)-max(y1,r*ch))
                if ox*oy>=cover*cw*ch: bits|=1<<(r*COLS+c)
    return bits

def main():
    mode,root=sys.argv[1:]
    if mode not in ('meta','mock'): raise ValueError('Invalid settings')
    root=pathlib.Path(root)
    if not root.is_dir() or root.stat().st_mode&0o077: raise ValueError('Private state directory required')
    v=strict_json(sys.stdin.buffer.read(400_000))
    if not isinstance(v,dict) or set(v)!={'concept','frame'}: raise ValueError('Invalid input')
    concept=clean(v['concept'],60)
    jpeg=base64.b64decode(v['frame'],validate=True)
    if not 4<=len(jpeg)<=131072 or not jpeg.startswith(b'\xff\xd8'): raise ValueError('Invalid JPEG')
    if mode=='mock':
        out={'source':'mock','concept':concept,'mask':'0'*12,'boxes':[]}
    else:
        with open(root/'vision.lock','w') as lock:
            fcntl.flock(lock,fcntl.LOCK_EX)
            key=private_secret(os.environ['XLENS_META_KEY_FILE'])
            reserve_slot(root,'vision-budget.json',DAILY,MIN_GAP)
            raw=post(RESPONSES,key,body(concept,jpeg),'application/json','text/event-stream',4*1024*1024)
        found=boxes(stream_text(raw))
        out={'source':'meta','concept':concept,'mask':f'{mask(found):012x}','boxes':[[round(b[0]/b[4],3),round(b[1]/b[5],3),round(b[2]/b[4],3),round(b[3]/b[5],3)] for b in found]}
    sys.stdout.write(json.dumps(out,ensure_ascii=True))

if __name__=='__main__':
    try: main()
    except Exception: sys.exit(1) # No images, keys or provider diagnostics in logs.
