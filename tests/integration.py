#!/usr/bin/env python3
"""Real socket/process tests with synthetic frames, private temporary state."""
import concurrent.futures
import http.client
import hashlib
import json
import os
import pathlib
import socket
import subprocess
import sys
import tempfile
import time
import unittest
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]/'tools'))
from simulate import JPEG
ROOT=pathlib.Path(__file__).resolve().parents[1]
BIN=ROOT/'target/release/x-lens'
VIEW='a'*64
DEVICE='b'*64
OPERATOR='c'*64

def req(method,path,body=None,token=VIEW,headers=None):
    c=http.client.HTTPConnection('127.0.0.1',8080,timeout=6)
    h={'Authorization':'Bearer '+token}
    h.update(headers or {})
    c.request(method,path,body,h)
    r=c.getresponse(); result=(r.status,r.read(),dict(r.getheaders())); c.close(); return result

def send(seq,body=JPEG,token=DEVICE):
    return req('POST','/api/frame/demo',body,token,{'X-Sequence':str(seq),'Content-Type':'image/jpeg'})[0]

class Platform(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(); self.root=pathlib.Path(self.tmp.name)
        cfg=self.root/'config'; cfg.write_text(f'viewer={VIEW}\noperator={OPERATOR}\ndevice.demo={DEVICE}\n'); cfg.chmod(0o600)
        self.proc=None; self.start()
    def start(self,mode='disabled'):
        env={**os.environ,'XLENS_RULE_MODE':mode}; env.pop('XLENS_AI_STATE_DIR',None); env.pop('XLENS_META_KEY_FILE',None)
        self.proc=subprocess.Popen([str(BIN),str(self.root/'config'),str(self.root/'data')],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,env=env)
        for _ in range(100):
            if self.proc.poll() is not None: self.fail('server did not start')
            try:
                if req('GET','/')[0]==200: return
            except OSError: time.sleep(.02)
        self.fail('server readiness deadline')
    def stop(self):
        if self.proc and self.proc.poll() is None: self.proc.terminate(); self.proc.wait(timeout=5)
    def tearDown(self): self.stop(); self.tmp.cleanup()
    def raw(self,data):
        with socket.create_connection(('127.0.0.1',8080),timeout=6) as s:
            s.sendall(data); result=b''
            while True:
                try: chunk=s.recv(65536)
                except ConnectionResetError: break
                if not chunk: break
                result+=chunk
            return result
    def test_rule_proposal_boundary(self):
        h={'X-Protocol':'1','Content-Type':'text/plain'}
        for key in [VIEW,DEVICE]:
            self.assertEqual(req('POST','/api/rule-draft/demo','five replays',key,h)[0],401)
        self.assertEqual(req('POST','/api/rule-draft/demo','x'*961,OPERATOR,h)[0],400)
        self.assertEqual(req('POST','/api/rule-draft/unknown','five replays',OPERATOR,h)[0],400)
        self.assertEqual(req('POST','/api/rule-draft/demo','five replays',OPERATOR,h)[0],502)
        self.assertEqual(json.loads(req('GET','/api/rules')[1]),[])
    def test_roundtrip_and_auth(self):
        self.assertEqual(req('GET','/api/devices',token=DEVICE)[0],401)
        self.assertEqual(send(1,token=VIEW),401)
        self.assertEqual(send(1),201)
        status,data,h=req('GET','/api/latest/demo')
        self.assertEqual((status,data),(200,JPEG)); self.assertEqual(h['Cache-Control'],'no-store'); self.assertEqual(h['X-Sequence'],'1')
        self.assertEqual(json.loads(req('GET','/api/devices')[1]),[{'id':'demo','sequence':1,'retained':1}])
        self.assertEqual(req('GET','/api/latest/../config')[0],404)
    def test_replay_restart_retention(self):
        for seq in range(1,41): self.assertEqual(send(seq),201)
        # Live frames stay in memory; disk keeps throttled snapshots (retention cap is a unit test).
        self.assertIn(len(list((self.root/'data/demo').glob('*.jpg'))),range(1,33))
        self.assertEqual(req('GET','/api/latest/demo')[2]['X-Sequence'],'40')
        self.assertEqual(json.loads(req('GET','/api/devices')[1])[0]['sequence'],40)
        self.assertEqual(send(40),409); self.assertEqual(send(1),409)
        self.stop(); self.start(); self.assertEqual(send(40),409); self.assertEqual(send(41),201)
    def test_live_stream(self):
        def open_stream(token=VIEW):
            c=http.client.HTTPConnection('127.0.0.1',8080,timeout=8)
            c.request('GET','/api/stream/demo',headers={'Authorization':'Bearer '+token}); return c,c.getresponse()
        c,r=open_stream(DEVICE); self.assertEqual(r.status,401); c.close()
        self.assertEqual(req('GET','/api/stream/unknown')[0],404)
        c,r=open_stream()
        self.assertEqual(r.status,200); self.assertTrue(r.getheader('Content-Type').startswith('multipart/x-mixed-replace'))
        def part():
            self.assertEqual(r.fp.readline(),b'--frame\r\n'); head={}
            while (line:=r.fp.readline())!=b'\r\n': k,v=line.decode().split(':',1); head[k.lower()]=v.strip()
            body=r.fp.read(int(head['content-length'])); self.assertEqual(r.fp.readline(),b'\r\n'); return head,body
        self.assertEqual(send(1),201)
        head,body=part(); self.assertEqual((head['content-type'],head['x-sequence'],body),('image/jpeg','1',JPEG))
        self.assertEqual(send(2),201); self.assertEqual(part()[0]['x-sequence'],'2')
        self.assertEqual(send(2),409) # Replays never reach viewers.
        head,body=part(); self.assertEqual((head['content-type'],body),('text/plain',b'')) # Idle heartbeat.
        c.close()
        self.assertEqual(req('GET','/api/devices')[0],200) # Streams do not occupy request workers.
    def test_motion_events_watches_and_ask(self):
        self.stop(); self.start('mock'); H={'X-Protocol':'1','Content-Type':'text/plain'}
        def frame(seq,motion):
            return req('POST','/api/frame/demo',JPEG,DEVICE,{'X-Sequence':str(seq),'Content-Type':'image/jpeg','X-Motion':str(motion)})
        watch='device=demo\nprompt=A parcel left at the door\ncooldown=60\nenabled=1'
        self.assertEqual(req('POST','/api/watches/parcel',watch,VIEW,H)[0],401) # Viewers cannot spend AI budget.
        self.assertEqual(req('POST','/api/watches/parcel',watch.replace('demo','ghost'),OPERATOR,H)[0],400)
        self.assertEqual(req('POST','/api/watches/parcel',watch,OPERATOR,H)[0],204)
        status,_,headers=frame(1,0); self.assertEqual((status,headers['X-Live-Viewers']),(201,'0'))
        for seq in range(2,8): self.assertEqual(frame(seq,200)[0],201)
        live=json.loads(req('GET','/api/activity')[1])
        self.assertEqual(live['devices'][0]['event'],1); self.assertIsNone(live['events'][0]['end'])
        self.assertEqual(req('GET','/api/keyframe/1')[0],404) # Keyframe once the event closes.
        self.assertEqual(frame(8,999)[0],201); self.assertEqual(frame(9,0)[0],201)
        for _ in range(100): # Ends after 5 s without frames; then the vision worker annotates it.
            event=json.loads(req('GET','/api/activity')[1])['events'][0]
            if event['ai']: break
            time.sleep(.1)
        self.assertEqual((event['id'],event['peak'],event['frames']),(1,999,9))
        self.assertEqual((event['ai']['source'],event['ai']['alerts']),('mock',[]))
        self.assertTrue(event['ai']['caption'].startswith('SIMULATED'))
        clip=req('GET','/api/clip/1')[1]; self.assertEqual(clip[:4],b'XLC1')
        self.assertEqual(req('GET','/api/keyframe/1')[:2],(200,JPEG))
        self.assertEqual(req('GET','/api/clip/1',token=DEVICE)[0],401)
        question='¿Pasó algo?'.encode()
        self.assertEqual(req('POST','/api/ask/demo',question,VIEW,H)[0],401)
        status,body,_=req('POST','/api/ask/demo',question,OPERATOR,{**H,'X-Utc-Offset':'-180'})
        self.assertEqual(status,200); answer=json.loads(body)
        self.assertEqual((answer['source'],answer['events']),('mock',[1])); self.assertIn('SIMULADO',answer['answer'])
        self.assertEqual(req('POST','/api/analyze/1','',OPERATOR,H)[0],202)
        # The key can be set from the dashboard, switches to live Meta, and is never returned.
        self.assertEqual(req('POST','/api/ai-key','short',OPERATOR,H)[0],400)
        self.stop(); self.start('auto')
        self.assertEqual(req('POST','/api/ai-key','meta-test-key-0123',VIEW,H)[0],401)
        self.assertEqual(req('POST','/api/ai-key','meta-test-key-0123',OPERATOR,H)[0],204)
        secret=self.root/'meta.secret'; self.assertEqual((secret.read_text(),secret.stat().st_mode&0o777),('meta-test-key-0123',0o600))
        status,body,_=req('GET','/api/activity'); ai=json.loads(body)['ai']
        self.assertEqual((ai['mode'],ai['key'],ai['model']),('meta',True,'muse-spark-1.3')); self.assertNotIn(b'meta-test-key',body)
        self.assertEqual(req('POST','/api/ai-key','',OPERATOR,H)[0],204); self.assertFalse(secret.exists())
        self.assertEqual(json.loads(req('GET','/api/activity')[1])['ai']['mode'],'mock')
        self.assertEqual(req('POST','/api/watches/parcel','delete=1',OPERATOR,H)[0],204)
        self.assertEqual(json.loads(req('GET','/api/watches')[1]),[])
    def test_zones_routines_labels_digest_and_voice(self):
        self.stop(); self.start('mock'); H={'X-Protocol':'1','Content-Type':'text/plain'}
        def frame(seq,motion):
            return req('POST','/api/frame/demo',JPEG,DEVICE,{'X-Sequence':str(seq),'Content-Type':'image/jpeg','X-Motion':str(motion)})
        # Ignore zones ride on every upload reply so the camera can apply them.
        self.assertEqual(frame(1,0)[2]['X-Motion-Mask'],'000000000000')
        self.assertEqual(req('POST','/api/zones/demo','mask=000000000003',VIEW,H)[0],401)
        for bad in ['mask=ffffffffffff','mask=3','zones=000000000003']: self.assertEqual(req('POST','/api/zones/demo',bad,OPERATOR,H)[0],400)
        self.assertEqual(req('POST','/api/zones/demo','mask=00000000ff03',OPERATOR,H)[0],204)
        self.assertEqual(frame(2,0)[2]['X-Motion-Mask'],'00000000ff03')
        self.assertEqual(json.loads(req('GET','/api/zones')[1]),{'demo':'00000000ff03'})
        status,body,_=req('POST','/api/zones-suggest/demo','tree',OPERATOR,H) # No key: SAM proposes nothing.
        self.assertEqual((status,json.loads(body)['source'],json.loads(body)['mask']),(200,'mock','000000000000'))
        # Routines: a window that already ended today with no motion is missed; an all-day one is seen.
        self.assertEqual(req('POST','/api/routines/wake','device=demo\nlabel=Gets up\nfrom=0\nto=1\noffset=0\nenabled=1',VIEW,H)[0],401)
        self.assertEqual(req('POST','/api/routines/wake','device=demo\nlabel=Gets up\nfrom=9\nto=1\noffset=0\nenabled=1',OPERATOR,H)[0],400)
        self.assertEqual(req('POST','/api/routines/wake','device=demo\nlabel=Gets up\nfrom=0\nto=1\noffset=0\nenabled=1',OPERATOR,H)[0],204)
        watch='device=demo\nprompt=A parcel left at the door\ncooldown=60\nenabled=1'
        self.assertEqual(req('POST','/api/watches/parcel',watch,OPERATOR,H)[0],204)
        for seq in range(3,9): self.assertEqual(frame(seq,200)[0],201)
        self.assertEqual(req('POST','/api/routines/day','device=demo\nlabel=Any activity\nfrom=0\nto=1440\noffset=0\nenabled=1',OPERATOR,H)[0],204)
        for _ in range(100):
            act=json.loads(req('GET','/api/activity')[1])
            if act['events'] and act['events'][0]['ai'] and len(act['notices'])>=(2 if time.gmtime().tm_hour or time.gmtime().tm_min>=2 else 1): break
            time.sleep(.1)
        status={r['id']:r['status'] for r in act['routines']}
        self.assertEqual(status['day'],'seen')
        if time.gmtime().tm_hour or time.gmtime().tm_min>=2: self.assertEqual(status['wake'],'missed')
        self.assertIn('missed' if status['wake']=='missed' else 'seen',[n['kind'] for n in act['notices']])
        # Labels: only for finished events and existing watches; they teach the next analyses.
        H2={**H}
        self.assertEqual(req('POST','/api/feedback/1','watch=parcel\nverdict=wrong',VIEW,H2)[0],401)
        self.assertEqual(req('POST','/api/feedback/1','watch=ghost\nverdict=wrong',OPERATOR,H2)[0],404)
        self.assertEqual(req('POST','/api/feedback/1','watch=parcel\nverdict=maybe',OPERATOR,H2)[0],400)
        self.assertEqual(req('POST','/api/feedback/99','watch=parcel\nverdict=wrong',OPERATOR,H2)[0],404)
        self.assertEqual(req('POST','/api/feedback/1','watch=parcel\nverdict=wrong',OPERATOR,H2)[0],204)
        act=json.loads(req('GET','/api/activity')[1])
        self.assertEqual(act['feedback'],[{'event':1,'watch':'parcel','right':False}])
        self.assertEqual(json.loads(req('GET','/api/watches')[1])[0]['learned'],1)
        self.assertEqual(req('POST','/api/feedback/1','watch=parcel\nverdict=clear',OPERATOR,H2)[0],204)
        self.assertEqual(json.loads(req('GET','/api/activity')[1])['feedback'],[])
        # Daily summary: one call, stored for the dashboard.
        self.assertEqual(req('POST','/api/digest','today',OPERATOR,H)[0],400) # Needs the local time zone.
        status,body,_=req('POST','/api/digest','today',OPERATOR,{**H,'X-Utc-Offset':'0'})
        self.assertEqual(status,200); digest=json.loads(body)
        self.assertEqual(digest['source'],'mock'); self.assertIn('SIMULATED',digest['summary']); self.assertEqual(digest['highlights'][0]['event'],1)
        self.assertEqual(json.loads(req('GET','/api/activity')[1])['digest'],digest)
        # Voice: operator only, WAV only, and without a key the browser recognizer is used instead.
        wav=b'RIFF'+b'\0'*60
        self.assertEqual(req('POST','/api/transcribe',wav,VIEW,{'X-Protocol':'1','Content-Type':'audio/wav'})[0],401)
        self.assertEqual(req('POST','/api/transcribe',wav,OPERATOR,{'X-Protocol':'1','Content-Type':'text/plain'})[0],400)
        self.assertEqual(req('POST','/api/transcribe',wav,OPERATOR,{'X-Protocol':'1','Content-Type':'audio/wav'})[0],503)
        big=b'\0'*(300*1024) # Larger than a frame, within the audio limit.
        self.assertEqual(req('POST','/api/transcribe',big,OPERATOR,{'X-Protocol':'1','Content-Type':'audio/wav'})[0],503)
        self.assertTrue(self.raw(b'POST /api/frame/demo HTTP/1.1\r\nHost: x\r\nContent-Length: 300000\r\n\r\n').startswith(b'HTTP/1.1 400'))
        # Everything survives a restart.
        self.stop(); self.start('mock'); act=json.loads(req('GET','/api/activity')[1])
        self.assertEqual((act['zones'],len(act['routines']),act['digest']),({'demo':'00000000ff03'},2,digest))
        self.assertEqual(req('POST','/api/routines/wake','delete=1',OPERATOR,H)[0],204)
    def test_favicon_probe_is_not_an_auth_failure(self):
        self.assertEqual(req('GET','/favicon.ico',token='')[0],404)
        self.assertEqual(json.loads(req('GET','/api/security')[1])['auth_failures'],0)
    def test_concurrent_duplicate(self):
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool: statuses=list(pool.map(lambda _:send(8),range(4)))
        self.assertEqual(sorted(statuses),[201,409,409,409])
    def test_strict_parser(self):
        samples=[b'Content-Length: 1\r\nContent-Length: 1',b'Transfer-Encoding: chunked',b'Content-Length: 131073',b'Content-Length: -1',b'Content-Length: +1',b'Host: again',b'Bad Header: x',b'Expect: 100-continue']
        for extra in samples:
            response=self.raw(b'POST /api/frame/demo HTTP/1.1\r\nHost: localhost\r\n'+extra+b'\r\n\r\n')
            self.assertTrue(response.startswith(b'HTTP/1.1 400'),extra)
        self.assertTrue(self.raw(b'GET / HTTP/1.1\r\nHost: '+b'x'*4096+b'\r\n\r\n').startswith(b'HTTP/1.1 400'))
    def test_sizes_and_sequences(self):
        self.assertEqual(send(0),400); self.assertEqual(send(-1),400); self.assertEqual(send(2**64),400)
        self.assertEqual(send(1,b'not a jpeg'),415)
        self.assertEqual(send(1,b'\xff\xd8'+b'\0'*(131072-4)+b'\xff\xd9'),201)
    def test_slow_header_deadline(self):
        with socket.create_connection(('127.0.0.1',8080),timeout=6) as s:
            start=time.monotonic(); s.sendall(b'GET / HTTP/1.1\r\n'); result=b''
            while b'\r\n' not in result:
                chunk=s.recv(4096)
                if not chunk: break
                result+=chunk
            self.assertTrue(result.startswith(b'HTTP/1.1 400')); self.assertLess(time.monotonic()-start,4)
    def test_crash_watermark_and_corruption(self):
        self.assertEqual(send(1),201); self.stop()
        (self.root/'data/demo/sequence').write_text('20')
        (self.root/'data/demo/pending').write_bytes(b'partial')
        self.start(); self.assertEqual(send(20),409); self.assertEqual(send(21),201)
        self.assertFalse((self.root/'data/demo/pending').exists())
        self.stop(); (self.root/'data/demo/sequence').write_text('broken')
        p=subprocess.run([str(BIN),str(self.root/'config'),str(self.root/'data')],capture_output=True,timeout=3)
        self.assertNotEqual(p.returncode,0)
    def test_security_telemetry_is_bounded_and_private(self):
        self.assertEqual(req('GET','/api/security',token=DEVICE)[0],401)
        self.assertEqual(send(1,token=VIEW),401)
        self.assertEqual(send(1),201); self.assertEqual(send(1),409)
        self.assertEqual(send(2,b'bad'),415)
        data=json.loads(req('GET','/api/security')[1])
        self.assertEqual(data['auth_failures'],2)
        self.assertEqual(data['devices'],[{'id':'demo','accepted':1,'replays':1,'invalid_frames':1}])
        self.assertNotIn(DEVICE,json.dumps(data)); self.assertNotIn(VIEW,json.dumps(data))
        self.stop(); self.start()
        self.assertEqual(json.loads(req('GET','/api/security')[1])['auth_failures'],0)
        self.assertEqual(send(1),409) # Telemetry resets; security watermark does not.
    def test_mock_incident_pipeline(self):
        self.assertEqual(send(1),201)
        for _ in range(5): self.assertEqual(send(1),409)
        subprocess.run([sys.executable,str(ROOT/'tools/analyze.py'),'--config',str(self.root/'config'),'--mode','incident','--mock','--report-dir',str(self.root/'data')],check=True,capture_output=True,timeout=10)
        self.assertEqual(req('GET','/api/review/demo',token=DEVICE)[0],401)
        status,body,_=req('GET','/api/review/demo');self.assertEqual(status,200)
        report=json.loads(body);self.assertEqual(report['source'],'mock')
        self.assertEqual(report['evidence']['sequence'],1)
        self.assertEqual(report['evidence']['image_sha256'],hashlib.sha256(JPEG).hexdigest())
        self.assertEqual(report['policy']['deterministic_recommendation'],'review')
        self.assertFalse(report['actions_executed']);self.assertIn('SIMULATED',report['assessment']['incident_summary'])
        self.assertFalse((self.root/'analysis-budget.json').exists())
        self.assertLessEqual(len(body),8192);self.assertNotIn(VIEW,body.decode())
    def rule(self, id='review', body=None, key=OPERATOR):
        body=body or 'device=demo\nsignal=replays\nthreshold=1\ncooldown=10\nenabled=1\n'
        return req('POST','/api/rules/'+id,body.encode(),key,{'X-Protocol':'1','Content-Type':'text/plain'})[0]
    def test_protocol_negotiation_and_device_sequence(self):
        self.assertEqual(req('GET','/api/capabilities',headers={'X-Protocol':'9'})[0],426)
        self.assertEqual(req('GET','/api/device-state/demo')[0],401)
        self.assertEqual(send(123),201)
        self.assertEqual(json.loads(req('GET','/api/device-state/demo',token=DEVICE)[1])['next_sequence'],'124')
        self.stop();self.start()
        self.assertEqual(json.loads(req('GET','/api/device-state/demo',token=DEVICE)[1])['next_sequence'],'124')
    def test_authenticated_rate_limit_and_recovery(self):
        statuses=[send(i) for i in range(1,121)]
        self.assertIn(429,statuses)
        time.sleep(.1)
        self.assertEqual(send(1000),201)
        self.assertEqual(req('GET','/api/devices')[0],200)
    def test_rule_capabilities_persistence_and_disable(self):
        self.assertEqual(self.rule(key=VIEW),401);self.assertEqual(self.rule(key=DEVICE),401)
        self.assertEqual(self.rule(),204)
        self.assertEqual(send(1),201);self.assertEqual(send(1),409)
        events=json.loads(req('GET','/api/events')[1]);self.assertEqual(len(events),1)
        self.assertEqual(events[0]['action'],'ui_event')
        self.assertEqual(len(json.loads(req('GET','/api/events')[1])),1)
        self.stop();self.start()
        self.assertEqual(json.loads(req('GET','/api/rules')[1])[0]['id'],'review')
        self.assertEqual(self.rule(body='device=demo\nsignal=replays\nthreshold=1\ncooldown=10\nenabled=0\n'),204)
        send(1);self.assertEqual(json.loads(req('GET','/api/events')[1]),[])
    def test_rules_reject_unsafe_or_ambiguous_input(self):
        for body in ['device=demo\nsignal=shell\nthreshold=1\ncooldown=10\nenabled=1\n',
                     'device=demo\nsignal=replays\nthreshold=0\ncooldown=10\nenabled=1\n',
                     'device=demo\nsignal=replays\nthreshold=1\ncooldown=10\nenabled=1\naction=execute\n',
                     'device=demo\nsignal=replays\nthreshold=1\ncooldown=10\nenabled=1\nenabled=0\n']:
            self.assertEqual(self.rule(body=body),400)
        self.assertEqual(self.rule(id='../x'),400)
        for i in range(16):self.assertEqual(self.rule(id='r'+str(i)),204)
        self.assertEqual(self.rule(id='too-many'),422)
        self.assertEqual(send(1),201) # Invalid rule capacity does not poison storage.
    def test_config_permissions(self):
        self.stop(); (self.root/'config').chmod(0o644)
        p=subprocess.run([str(BIN),str(self.root/'config'),str(self.root/'data')],capture_output=True,timeout=3)
        self.assertNotEqual(p.returncode,0)
if __name__=='__main__': unittest.main(verbosity=2)
