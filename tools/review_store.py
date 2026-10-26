"""Private bounded local report and pre-call budget ledger; operator-owned files."""
import json
import os
import pathlib
import time
from security_review import strict_json

def atomic_json(path,value):
    data=json.dumps(value,ensure_ascii=True,separators=(',',':')).encode()
    if len(data)>8192: raise ValueError('Local report exceeds 8 KiB')
    temp=path.with_suffix('.pending')
    fd=os.open(temp,os.O_CREAT|os.O_TRUNC|os.O_WRONLY,0o600)
    with os.fdopen(fd,'wb') as f: f.write(data);f.flush();os.fsync(f.fileno())
    os.replace(temp,path)
    d=os.open(path.parent,os.O_RDONLY)
    try:os.fsync(d)
    finally:os.close(d)

def reserve_budget(root,daily,cooldown,now=None):
    if not 1<=daily<=100 or not 60<=cooldown<=86400: raise ValueError('Budget must be 1..100/day; cooldown 60..86400 seconds')
    now=int(time.time()) if now is None else now
    path=pathlib.Path(root)/'analysis-budget.json'
    if path.exists():
        if path.stat().st_size>1024: raise ValueError('Invalid budget ledger; manual recovery required')
        v=strict_json(path.read_text())
        if not isinstance(v,dict) or set(v)!={'day','count','last'} or any(type(x) is not int or x<0 for x in v.values()): raise ValueError('Invalid budget ledger; manual recovery required')
        if now<v['last'] or now-v['last']<cooldown: raise ValueError('Inference cooldown or backward clock; no request sent')
    else:v={'day':now//86400,'count':0,'last':0}
    count=v['count'] if v['day']==now//86400 else 0
    if count>=daily: raise ValueError('Daily inference budget exhausted; no request sent')
    # Reserve before network I/O. Unknown/failed calls still consume a slot.
    atomic_json(path,{'day':now//86400,'count':count+1,'last':now})

def save_report(root,device,report):
    directory=pathlib.Path(root)/device
    if not directory.is_dir() or directory.is_symlink() or directory.stat().st_mode&0o077:
        raise ValueError('Report directory must be an existing private server device directory')
    atomic_json(directory/'review.json',report)
