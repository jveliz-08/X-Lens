"""Bounded advisory policy. Nothing here changes authentication or runs actions."""
import json
import re
RISKS=('low','medium','high')
RECOMMENDATIONS=('observe','review','step_up_review','quarantine_review')
REASONS=('no_signal','auth_failures','replay_attempts','invalid_frames')
OBJECTS=('person','vehicle','animal','package','other')

def strict_json(text):
    def pairs(items):
        result={}
        for key,value in items:
            if key in result: raise ValueError('Duplicate JSON key')
            result[key]=value
        return result
    return json.loads(text,object_pairs_hook=pairs)

def telemetry(value):
    if not isinstance(value,dict) or set(value)!={'uptime_seconds','auth_failures','devices'}: raise ValueError('Unexpected telemetry shape')
    def number(v):
        if type(v) is not int or not 0<=v<=2**64-1: raise ValueError('Invalid counter')
        return v
    rows=value['devices']
    if not isinstance(rows,list) or not 1<=len(rows)<=16: raise ValueError('Invalid device count')
    clean={'uptime_seconds':number(value['uptime_seconds']),'auth_failures':number(value['auth_failures']),'devices':[]}
    for i,row in enumerate(rows):
        if not isinstance(row,dict) or set(row)!={'id','accepted','replays','invalid_frames'}: raise ValueError('Unexpected device telemetry')
        if not isinstance(row['id'],str) or not re.fullmatch('[a-z0-9-]{1,24}',row['id']): raise ValueError('Invalid device ID')
        # Only numbered slots leave the host; no location-bearing names or raw logs.
        clean['devices'].append({'slot':i+1,**{k:number(row[k]) for k in ('accepted','replays','invalid_frames')}})
    return clean

def validate_security(value):
    if not isinstance(value,dict) or set(value)!={'risk','recommendation','reason_codes'}: raise ValueError('Unexpected model output')
    if value['risk'] not in RISKS or value['recommendation'] not in RECOMMENDATIONS: raise ValueError('Invalid recommendation')
    codes=value['reason_codes']
    if not isinstance(codes,list) or not 1<=len(codes)<=4 or any(c not in REASONS for c in codes): raise ValueError('Invalid reason codes')
    return value

def validate_image(value):
    if not isinstance(value,dict) or set(value)!={'objects','summary','uncertain'}: raise ValueError('Unexpected image output')
    if type(value['uncertain']) is not bool: raise ValueError('Invalid uncertainty')
    if not isinstance(value['summary'],str) or len(value['summary'])>400 or any(ord(c)<32 for c in value['summary']): raise ValueError('Invalid summary')
    if not isinstance(value['objects'],list) or len(value['objects'])>16 or any(c not in OBJECTS for c in value['objects']): raise ValueError('Invalid objects')
    return value

def policy(clean,model=None):
    reasons=[]
    if clean['auth_failures']>=10: reasons.append('auth_failures')
    if any(d['replays']>=5 for d in clean['devices']): reasons.append('replay_attempts')
    if any(d['invalid_frames']>=3 for d in clean['devices']): reasons.append('invalid_frames')
    # Model cannot escalate privileges, trigger a block, or suppress deterministic evidence.
    level='review' if reasons else 'observe'
    return {'policy':'advisory-v1','deterministic_recommendation':level,'evidence':reasons or ['no_signal'],
            'model_advice':validate_security(model) if model is not None else None,
            'human_approval_required':True,'automatic_action':False,
            'note':'Counters are cumulative since process start; signals are not proof of compromise. Review before any step-up or quarantine.'}
