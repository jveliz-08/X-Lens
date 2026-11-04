"""Raw HTTPS calls for Meta endpoints that are not JSON-in/JSON-out (SSE, multipart).
Same rules as analyze.call_provider: fixed endpoint, no redirects, absolute deadline,
bounded response, never retried."""
import signal
import socket
import urllib.error
import urllib.request
from analyze import NoRedirect, UnknownOutcome, endpoint

RESPONSES='https://api.meta.ai/v1/responses'
TRANSCRIBE='https://api.meta.ai/v1/asr/transcribe'

def post(url,key,body,content_type,accept,limit,opener=None):
    endpoint(url)
    request=urllib.request.Request(url,body,{'Authorization':'Bearer '+key,'Content-Type':content_type,'Accept':accept},method='POST')
    opener=opener or urllib.request.build_opener(NoRedirect)
    def deadline(signum,frame): raise TimeoutError('provider absolute deadline')
    previous=signal.signal(signal.SIGALRM,deadline)
    signal.alarm(25)
    try:
        with opener.open(request,timeout=20) as r:
            data=r.read(limit+1)
            if len(data)>limit: raise UnknownOutcome('Provider response too large; do not retry automatically.')
            return data
    except (TimeoutError,socket.timeout,urllib.error.URLError,OSError,ValueError) as e:
        raise UnknownOutcome('Provider outcome unknown or rejected; no retry was made.') from e
    finally:
        signal.alarm(0)
        signal.signal(signal.SIGALRM,previous)
