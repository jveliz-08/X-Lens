#!/usr/bin/env python3
"""Real socket/process tests with synthetic frames, private temporary state."""
import concurrent.futures
import http.client
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
        cfg=self.root/'config'; cfg.write_text(f'viewer={VIEW}\ndevice.demo={DEVICE}\n'); cfg.chmod(0o600)
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
    def test_roundtrip_and_auth(self):
        self.assertEqual(req('GET','/api/devices',token=DEVICE)[0],401)
        self.assertEqual(send(1,token=VIEW),401)
        self.assertEqual(send(1),201)
        status,data,h=req('GET','/api/latest/demo')
        self.assertEqual((status,data),(200,JPEG)); self.assertEqual(h['Cache-Control'],'no-store')
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
    def test_config_permissions(self):
        self.stop(); (self.root/'config').chmod(0o644)
        p=subprocess.run([str(BIN),str(self.root/'config'),str(self.root/'data')],capture_output=True,timeout=3)
        self.assertNotEqual(p.returncode,0)
if __name__=='__main__': unittest.main(verbosity=2)
