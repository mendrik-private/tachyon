"""Validate the exact AT-SPI button contract used by the native find check."""
import unittest

from find_check import validate_controls


class FindControlsTest(unittest.TestCase):
    def controls(self):
        return [dict(role='button', name=name, actions=['click'])
                for name in ('Previous result', 'Next result', 'Close find')]

    def test_named_actionable_buttons(self):
        validate_controls(self.controls())

    def test_missing_name_role_or_action_fails(self):
        for field, value in [('name', ''), ('role', 'label'), ('actions', [])]:
            with self.subTest(field=field):
                nodes = self.controls()
                nodes[0][field] = value
                with self.assertRaises(RuntimeError):
                    validate_controls(nodes)

    def test_duplicate_control_fails(self):
        nodes = self.controls()
        nodes.append(nodes[0].copy())
        with self.assertRaises(RuntimeError):
            validate_controls(nodes)


if __name__ == '__main__':
    unittest.main()
