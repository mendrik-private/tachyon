"""Safety boundaries for the native accessibility test harness."""
import os
import subprocess
import unittest
from unittest.mock import patch

from accessibility_probe import SnapshotChanged, private_bus, snapshot, stable_traversal


class AccessibilityProbeTests(unittest.TestCase):
    def test_changed_tree_retries_without_partial_nodes(self):
        nodes, objects = [], {}
        calls = 0

        def visit(root, parent, depth):
            nonlocal calls
            calls += 1
            self.assertEqual(nodes, [])
            self.assertEqual(objects, {})
            nodes.append(calls)
            objects['root'] = calls
            if calls == 1:
                raise SnapshotChanged('removed child')

        with patch('accessibility_probe.time.sleep'):
            stable_traversal(visit, object(), nodes, objects)
        self.assertEqual(nodes, [2])
        self.assertEqual(objects, {'root': 2})

    def test_changing_tree_has_bounded_retries_and_other_errors_propagate(self):
        for error, expected_calls in [(SnapshotChanged('removed'), 4), (ValueError('bad'), 1)]:
            with self.subTest(error=error), patch('accessibility_probe.time.sleep'):
                with patch(__name__ + '.snapshot', side_effect=error) as visit:
                    with self.assertRaises(type(error)):
                        stable_traversal(visit, object(), [], {})
                    self.assertEqual(visit.call_count, expected_calls)

    def test_only_disappeared_accessible_dbus_errors_are_retried(self):
        for message, calls in [("Unknown object '/org/a11y/atspi/accessible/0/123'", 4),
                               ("Unknown object '/org/a11y/atspi/cache'", 1),
                               ('Permission denied', 1)]:
            error = RuntimeError(message)
            error.domain, error.code, error.message = 'atspi_error', 1, message
            with self.subTest(message=message), patch('accessibility_probe.time.sleep'):
                with patch(__name__ + '.snapshot', side_effect=error) as visit:
                    with self.assertRaises(RuntimeError):
                        stable_traversal(visit, object(), [], {})
                    self.assertEqual(visit.call_count, calls)

    def test_probe_refuses_unowned_session(self):
        for environment in ({}, {"DBUS_SESSION_BUS_ADDRESS": "unix:path=/not-a-private-bus"},
                            {"DBUS_SESSION_BUS_ADDRESS": "one", "MINERAL_PRIVATE_ATSPI_BUS": "two"}):
            with self.subTest(environment=environment), patch.dict(os.environ, environment, clear=True):
                with self.assertRaisesRegex(RuntimeError, "harness-owned private session bus"):
                    snapshot(0)

    def test_private_bus_activation_is_limited_and_does_not_outlive_context(self):
        with private_bus(os.environ, enabled=False) as environment:
            self.assertEqual(environment["GSETTINGS_BACKEND"], "memory")
            self.assertEqual(environment["GIO_USE_VFS"], "local")
            command = ["gdbus", "call", "--session", "--dest", "org.freedesktop.DBus",
                       "--object-path", "/org/freedesktop/DBus", "--method",
                       "org.freedesktop.DBus.ListActivatableNames"]
            active = subprocess.run(command, env=environment, capture_output=True, text=True, timeout=5, check=True)
            self.assertIn("org.a11y.Bus", active.stdout)
            self.assertNotIn("org.gtk.vfs", active.stdout)
            self.assertNotIn("org.freedesktop.portal", active.stdout)
        stopped = subprocess.run(command, env=environment, capture_output=True, text=True, timeout=5)
        self.assertNotEqual(stopped.returncode, 0)


if __name__ == "__main__":
    unittest.main()
