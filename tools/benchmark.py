#!/usr/bin/env python3
"""Reproducible loopback benchmark. No TLS or ESP32 performance inference."""
import json
import pathlib
import statistics
import subprocess
import sys
import tempfile
import time
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]/'tests'))
from integration import BIN,VIEW,DEVICE,req,send,JPEG

def main():
    with tempfile.TemporaryDirectory() as temp:
        root=pathlib.Path(temp); cfg=root/'config'; cfg.write_text(f'viewer={VIEW}\ndevice.demo={DEVICE}\n'); cfg.chmod(0o600)
        p=subprocess.Popen([str(BIN),str(cfg),str(root/'data')],stderr=subprocess.DEVNULL)
        try:
            for _ in range(100):
                if p.poll() is not None: raise RuntimeError('server unavailable; stop other instance first')
                try: req('GET','/'); break
                except OSError: time.sleep(.02)
            # Original JPEG + valid JPEG COM segment; no image library required.
            frame=JPEG[:-2]+b'\xff\xfe'+(60002).to_bytes(2,'big')+b'x'*60000+JPEG[-2:]
            durations=[]; started=time.monotonic()
            for seq in range(1,201):
                t=time.monotonic(); status=send(seq,frame); durations.append((time.monotonic()-t)*1000)
                if status!=201: raise RuntimeError(status)
            elapsed=time.monotonic()-started
            mem={line.split(':')[0]:line.split(':')[1].strip() for line in pathlib.Path(f'/proc/{p.pid}/status').read_text().splitlines() if line.startswith(('VmRSS:','VmHWM:'))}
            output={'frames':200,'jpeg_bytes':len(frame),'elapsed_seconds':round(elapsed,3),'requests_per_second':round(200/elapsed,2),'latency_median_ms':round(statistics.median(durations),3),'latency_p95_ms':round(sorted(durations)[189],3),'binary_bytes':BIN.stat().st_size,'retained_frames':len(list((root/'data/demo').glob('*.jpg'))),'retained_jpeg_bytes':sum(f.stat().st_size for f in (root/'data/demo').glob('*.jpg')),'process_memory':mem,'scope':'single client, loopback HTTP, fsync enabled, synthetic JPEG; not TLS/Wi-Fi/ESP32 throughput'}
            print(json.dumps(output,indent=2))
        finally: p.terminate(); p.wait(timeout=5)
if __name__=='__main__': main()
