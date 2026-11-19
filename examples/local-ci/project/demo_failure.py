import unittest

from test_calculator import add


class FailingExample(unittest.TestCase):
    def test_add(self):
        self.assertEqual(add(2, 3), 6, "Intentional failure for the Local CI example")
