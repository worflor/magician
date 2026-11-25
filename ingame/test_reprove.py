"""Tests for reprove.py, no game needed: python test_reprove.py

What a pass is called (ready, look, failed) and why, what the report says, and what `accept` refuses. The sessions themselves are mod_session.py's, tested there.
"""
import json
import os
import shutil
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import reprove as R  # noqa: E402
import mod_session as S  # noqa: E402


class Classify(unittest.TestCase):
    def good(self, **over):
        f = {"session_rc": 0, "runs": [("angles", 0), ("motion", 0)], "dumps": [], "base": True, "new_warnings": 0}
        f.update(over)
        return f

    def test_a_pass_that_ran_whole_and_adds_no_warning_is_ready(self):
        self.assertEqual(R.classify(self.good()), ("ready", []))

    def test_a_failed_scenario_a_crash_or_a_dead_session_is_failed(self):
        self.assertEqual(R.classify(self.good(runs=[("angles", 0), ("motion", 8)]))[0], "failed")
        self.assertIn("scenario motion failed (code 8)", R.classify(self.good(runs=[("motion", 8)]))[1])
        self.assertEqual(R.classify(self.good(dumps=["a.mdmp"]))[0], "failed")
        self.assertEqual(R.classify(self.good(session_rc=7))[0], "failed")
        self.assertEqual(R.classify(self.good(runs=[])), ("failed", ["no scenario ran"]))

    def test_anything_that_differs_or_could_not_be_compared_is_a_look_and_says_why(self):
        state, why = R.classify(self.good(new_warnings=2))
        self.assertEqual((state, why), ("look", ["2 warning(s) vanilla does not print"]))
        self.assertIn("older build", R.classify(self.good(base=False))[1][0])
        self.assertIn("no vanilla baseline", R.classify(self.good(base=None, new_warnings=None))[1][0])


class Passes(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.mkdtemp()

    def tearDown(self):
        shutil.rmtree(self.d, ignore_errors=True)

    def a_pass(self, slug, name, **state):
        p = os.path.join(self.d, slug, name)
        os.makedirs(p)
        st = {"out": p, "slug": slug, "runs": [{"name": "angles", "rc": 0}], "mod": os.path.join(self.d, slug + ".toml")}
        st.update(state)
        with open(os.path.join(p, "session.json"), "w", encoding="utf-8") as f:
            json.dump(st, f)
        return p

    def test_a_pass_folder_is_named_for_the_mod_the_build_and_the_time(self):
        self.assertEqual(R.pass_dir(self.d, "example-skin", "6866", "20261124-1000", vanilla=True),
                         os.path.join(self.d, "example-skin", "6866-20261124-1000-vanilla"))
        self.assertEqual(R.pass_dir(self.d, "example-skin", "6866", "20261124-1000"),
                         os.path.join(self.d, "example-skin", "6866-20261124-1000"))

    def test_the_report_puts_the_worst_first_and_ends_each_mod_with_how_to_record_it(self):
        results = [{"slug": "b-ready", "state": "ready", "reasons": [], "pass": r"D:\p\b", "sheets": ["a_sheet.png"]},
                   {"slug": "a-look", "state": "look", "reasons": ["2 warning(s) vanilla does not print"], "pass": r"D:\p\a",
                    "sheets": [], "new": ["Failed loading resource x"]},
                   {"slug": "c-failed", "state": "failed", "reasons": ["scenario motion failed (code 8)"], "pass": r"D:\p\c", "sheets": []}]
        text = R.report("6866", results, "20261124-1000")
        self.assertLess(text.index("## c-failed: failed"), text.index("## a-look: look"))
        self.assertLess(text.index("## a-look: look"), text.index("## b-ready: ready"))
        self.assertIn("new warning: Failed loading resource x", text)
        self.assertIn('reprove.py accept "D:\\p\\a"', text)
        self.assertNotIn('accept "D:\\p\\c"', text, "a failed pass is not offered for acceptance")
        self.assertIn("Nothing here proves a mod", text)

    def test_accept_refuses_what_is_not_a_good_pass_of_the_mod(self):
        def code(path):
            try:
                R.accept(path)
            except S.Stop as e:
                return e.code, str(e)
            return 0, ""
        self.assertEqual(code(os.path.join(self.d, "nothing"))[0], 2)
        toml = os.path.join(self.d, "example-skin.toml")
        open(toml, "w").close()
        c, why = code(self.a_pass("example-skin", "v", vanilla=True))
        self.assertEqual(c, 2)
        self.assertIn("vanilla pass", why)
        c, why = code(self.a_pass("example-skin", "f", runs=[{"name": "motion", "rc": 8}]))
        self.assertEqual(c, 1)
        self.assertIn("motion", why)
        self.assertEqual(code(self.a_pass("example-skin", "n", runs=[]))[0], 1)


if __name__ == "__main__":
    unittest.main()
