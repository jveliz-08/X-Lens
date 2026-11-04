#!/usr/bin/env python3
"""Push-to-talk transcription with Meta Muse Voice Transcribe. Invoked only by the server:

    python3 tools/voice.py <private state dir> <language code>   (WAV on stdin)

The browser records 16 kHz mono 16-bit PCM WAV (the documented format). One budgeted
multipart call to /v1/asr/transcribe in PUSH_TO_TALK mode; prints {"source","text"}.
Audio is never stored. Never retried.
"""
import fcntl
import json
import os
import pathlib
import struct
import sys
import uuid
from analyze import private_secret
from meta_media import TRANSCRIBE, post
from review_store import reserve_slot
from security_review import strict_json
from vision import LANGUAGES, clean

DAILY=200
MAX_WAV=512*1024

def check_wav(data):
    """RIFF/WAVE, PCM, mono, 16-bit, 16 or 24 kHz, at most ~16 s."""
    if len(data)<44 or len(data)>MAX_WAV or data[:4]!=b'RIFF' or data[8:12]!=b'WAVE' or data[12:16]!=b'fmt ': raise ValueError('Not a WAV file')
    fmt,channels,rate,_,_,bits=struct.unpack('<HHIIHH',data[20:36])
    if fmt!=1 or channels!=1 or bits!=16 or rate not in (16000,24000): raise ValueError('Unsupported WAV format')
    return data

def body(wav,language):
    boundary='xlens-'+uuid.uuid4().hex
    settings={'model':'muse-voice-transcribe-1.0','audioEncoding':'WAV','mode':'PUSH_TO_TALK'}
    if language in LANGUAGES: settings['languageBias']=[LANGUAGES[language]]
    parts=[f'--{boundary}\r\nContent-Disposition: form-data; name="request"\r\nContent-Type: application/json\r\n\r\n'.encode()+json.dumps(settings).encode(),
           f'--{boundary}\r\nContent-Disposition: form-data; name="audio"; filename="speech.wav"\r\nContent-Type: audio/wav\r\n\r\n'.encode()+wav]
    return b'\r\n'.join(parts)+f'\r\n--{boundary}--\r\n'.encode(),f'multipart/form-data; boundary={boundary}'

def transcript(raw):
    v=strict_json(raw)
    if not isinstance(v,dict) or not isinstance(v.get('transcript'),str): raise ValueError('Invalid transcription')
    return v['transcript']

def main():
    root,language=sys.argv[1:]
    root=pathlib.Path(root)
    if not root.is_dir() or root.stat().st_mode&0o077: raise ValueError('Private state directory required')
    wav=check_wav(sys.stdin.buffer.read(MAX_WAV+1))
    data,kind=body(wav,language)
    with open(root/'vision.lock','w') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        key=private_secret(os.environ['XLENS_META_KEY_FILE'])
        reserve_slot(root,'voice-budget.json',DAILY,0)
        raw=post(TRANSCRIBE,key,data,kind,'application/json',65536)
    text=transcript(raw).strip()
    sys.stdout.write(json.dumps({'source':'meta','text':clean(text,300) if text else ''},ensure_ascii=True))

if __name__=='__main__':
    try: main()
    except Exception: sys.exit(1) # No audio, keys or provider diagnostics in logs.
