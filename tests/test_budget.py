import pathlib
import sys
import tempfile
import unittest
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]/'tools'))
from review_store import reserve_budget
class Budget(unittest.TestCase):
    def test_cooldown_daily_limit_and_pre_reserved_unknown_call(self):
        with tempfile.TemporaryDirectory() as td:
            reserve_budget(td,2,60,now=1000)
            with self.assertRaises(ValueError): reserve_budget(td,2,60,now=1001)
            # A request could time out here; reservation is still consumed.
            reserve_budget(td,2,60,now=1060)
            with self.assertRaises(ValueError): reserve_budget(td,2,60,now=1120)
            reserve_budget(td,2,60,now=86400)
    def test_corruption_and_backward_clock_fail_closed(self):
        with tempfile.TemporaryDirectory() as td:
            reserve_budget(td,2,60,now=1000)
            with self.assertRaises(ValueError): reserve_budget(td,2,60,now=900)
            pathlib.Path(td,'analysis-budget.json').write_text('broken')
            with self.assertRaises(ValueError): reserve_budget(td,2,60,now=2000)
if __name__=='__main__':unittest.main(verbosity=2)
