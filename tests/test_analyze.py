import io
import json
import pathlib
import sys
import unittest
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]/'tools'))
from analyze import call_provider,endpoint,payload,UnknownOutcome,MAX_RESPONSE
from simulate import JPEG
class Fake:
    def __init__(self,response=None): self.calls=0; self.response=response
    def open(self,request,timeout):
        self.calls+=1
        if self.response is None: raise TimeoutError('synthetic timeout')
        return io.BytesIO(self.response)
class Adapter(unittest.TestCase):
    def test_no_http_or_embedded_credentials(self):
        for url in ['http://example.com','https://key@example.com','https://example.com?key=secret','https://example.com#x']:
            with self.assertRaises(ValueError): endpoint(url)
    def test_payload_bounds(self):
        self.assertEqual(json.loads(payload('demo',JPEG))['device'],'demo')
        with self.assertRaises(ValueError): payload('demo',b'x'*131073)
    def test_timeout_never_retries(self):
        f=Fake()
        with self.assertRaises(UnknownOutcome): call_provider('https://example.com','synthetic',b'{}',f)
        self.assertEqual(f.calls,1)
    def test_response_limit(self):
        f=Fake(b'x'*(MAX_RESPONSE+1))
        with self.assertRaises(UnknownOutcome): call_provider('https://example.com','synthetic',b'{}',f)
        self.assertEqual(f.calls,1)
    def test_response(self):
        self.assertEqual(call_provider('https://example.com','synthetic',b'{}',Fake(b'{"objects":[]}')),{'objects':[]})
if __name__=='__main__': unittest.main(verbosity=2)
