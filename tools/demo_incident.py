#!/usr/bin/env python3
"""Local-only synthetic incident demo. Never contacts Meta or a real camera."""
import argparse
import json
import pathlib
import subprocess
import sys
import time
import urllib.error
from simulate import upload

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--config',default='runtime/config.secret');p.add_argument('--report-dir',default='runtime/frames');args=p.parse_args()
    config=pathlib.Path(args.config)
    if config.stat().st_mode&0o077: p.error('Config must be mode 600')
    cfg=dict(line.split('=',1) for line in config.read_text().splitlines() if line and not line.startswith('#'))
    key=cfg['device.demo'];seq=time.time_ns()//1000;url='http://127.0.0.1:8080/api/frame/demo'
    upload(url,key,seq)
    for _ in range(5):
        try:upload(url,key,seq)
        except urllib.error.HTTPError as e:
            if e.code!=409:raise
    subprocess.run([sys.executable,str(pathlib.Path(__file__).with_name('analyze.py')),'--config',args.config,'--device','demo','--mode','incident','--mock','--report-dir',args.report_dir],check=True)
    print('Synthetic incident saved. Unlock/refresh the dashboard; the review is explicitly labeled SIMULATED AI.')
if __name__=='__main__':main()
