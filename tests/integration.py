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
    def start(self):
        self.proc=subprocess.Popen([str(BIN),str(self.root/'config'),str(self.root/'data')],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
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
        self.assertEqual(len(list((self.root/'data/demo').glob('*.jpg'))),32)
        self.assertEqual(send(40),409); self.assertEqual(send(1),409)
        self.stop(); self.start(); self.assertEqual(send(40),409); self.assertEqual(send(41),201)
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
