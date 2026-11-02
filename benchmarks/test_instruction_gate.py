"""Tests for the instruction-count gate's decisions."""

import unittest

from instruction_gate import lowered, parse_instructions, regressions


class InstructionGateTests(unittest.TestCase):
    def test_reads_the_count_from_every_report_spelling(self):
        for report, expected in [
            ("==1== I   refs:      1,234,567\n", 1234567),
            ("==1== I refs: 42\n", 42),
            ("noise\n==2== I        refs:  9,000\n==2==\n", 9000),
        ]:
            self.assertEqual(parse_instructions(report), expected)
        for report in ["", "==1== D refs: 12\n"]:
            with self.assertRaises(ValueError):
                parse_instructions(report)

    def test_flags_a_rise_and_a_missing_case_but_not_noise_or_a_fall(self):
        recorded = {"fib": 2.0, "tak": 1.5}
        self.assertEqual(regressions({"fib": 2.0, "tak": 1.2}, recorded), [])
        self.assertEqual(regressions({"fib": 2.01}, recorded), [])
        self.assertEqual(regressions({"fib": 2.1}, recorded), ["fib: 2.100 is above the recorded 2.000"])
        self.assertEqual(regressions({"new": 3.0}, recorded), ["new: 3.000 has no recorded ratio; run --record"])

    def test_recorded_ratios_only_go_down(self):
        self.assertEqual(lowered({"fib": 2.5, "tak": 1.2, "new": 3.14159}, {"fib": 2.0, "tak": 1.5}), {"fib": 2.0, "new": 3.142, "tak": 1.2})


if __name__ == "__main__":
    unittest.main()
