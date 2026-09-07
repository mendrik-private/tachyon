"""The native equation oracle must reject missing structure, not just text."""
import unittest
from math_accessibility_check import mathml


class MathAccessibilityTests(unittest.TestCase):
    def test_nested_fraction_keeps_numerator_and_denominator_order(self):
        nodes = [
            dict(parent=None, name='source', attributes={'tag': 'math'}),
            dict(parent=0, name='', attributes={'tag': 'mfrac'}),
            dict(parent=1, name='37', attributes={'tag': 'mn'}),
            dict(parent=1, name='41', attributes={'tag': 'mn'}),
        ]
        fraction = mathml(nodes, 0).find('mfrac')
        self.assertEqual([child.text for child in fraction], ['37', '41'])
        nodes[1]['attributes'] = {}
        with self.assertRaisesRegex(RuntimeError, 'native math tag'):
            mathml(nodes, 0)

    def test_token_content_is_text_not_xml(self):
        nodes = [dict(parent=None, name='<bad>&', attributes={'tag': 'mtext'})]
        self.assertEqual(mathml(nodes, 0).text, '<bad>&')
