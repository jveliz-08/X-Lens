#!/usr/bin/env python3
"""Upload an original synthetic 320x240 JPEG; never accesses a camera."""
import argparse
import pathlib
import time
import urllib.request
# Original test card; regenerate with the optional maintainer tool make_fixture.py.
JPEG = (pathlib.Path(__file__).resolve().parents[1] / 'tests/fixtures/synthetic.jpg').read_bytes()
def upload(url, key, seq, jpeg=JPEG):
    req = urllib.request.Request(url, jpeg, {'Authorization': f'Bearer {key}', 'Content-Type':'image/jpeg', 'X-Sequence':str(seq)}, method='POST')
    # URL fixed to localhost in CLI. Never carry credentials through redirects.
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *args, **kwargs): return None
    with urllib.request.build_opener(NoRedirect).open(req, timeout=5) as r:
        return r.status

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--config', default='runtime/config.secret')
    p.add_argument('--count', type=int, default=1)
    p.add_argument('--start', type=int, default=None, help='Default: wall-clock microseconds; explicit sequence useful for replay tests')
    args = p.parse_args()
    if not 1 <= args.count <= 1000: p.error('count must be 1..1000')
    cfg = dict(line.split('=',1) for line in pathlib.Path(args.config).read_text().splitlines() if line and not line.startswith('#'))
    start = args.start if args.start is not None else time.time_ns() // 1000
    for i in range(args.count): print(f'sequence={start+i} status={upload("http://127.0.0.1:8080/api/frame/demo",cfg["device.demo"],start+i)}')
if __name__ == '__main__': main()
