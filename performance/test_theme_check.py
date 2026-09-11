import os
import subprocess
import unittest
import tempfile
from pathlib import Path

from accessibility_probe import private_bus
from theme_check import PALETTES, check_image, editor_state, evaluate_counts, semantic_geometry
from theme_portal import require_private_bus, set_appearance, settings_portal


class ThemeCheckTests(unittest.TestCase):
    def test_chrome_color_elsewhere_cannot_substitute_for_title_bar(self):
        from PIL import Image
        colors = PALETTES['dark']
        rgb = lambda color: tuple((color >> shift) & 255 for shift in (16, 8, 0))
        image = Image.new('RGB', (100, 100), rgb(colors['page']))
        image.paste(rgb(colors['heading']), (0, 40, 20, 45))
        image.paste(rgb(colors['text']), (0, 50, 20, 55))
        image.paste(rgb(colors['title_bar']), (0, 60, 100, 70))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'appearance.png'
            image.save(path)
            self.assertFalse(check_image(path, 'dark')['passes'])
            image.paste(rgb(colors['title_bar']), (0, 0, 100, 34))
            image.save(path)
            self.assertTrue(check_image(path, 'dark')['passes'])

    def test_editor_state_requires_focus_evidence(self):
        editor = {"role": "entry", "name": "Markdown document editor", "states": ["focused"]}
        state = editor_state({"nodes": [editor]})
        self.assertEqual(state, {"focused": True})
        self.assertNotEqual(state, editor_state({"nodes": [{**editor, "states": []}]}))
        with self.assertRaises(RuntimeError):
            editor_state({"nodes": []})

    def test_pixels_require_matching_document_chrome_and_ink(self):
        for mode, colors in PALETTES.items():
            counts = {tuple((color >> shift) & 255 for shift in (16, 8, 0)): 3000
                      for color in colors.values()}
            self.assertTrue(evaluate_counts(counts, 10000, mode)["passes"])
            other = "dark" if mode == "light" else "light"
            self.assertFalse(evaluate_counts(counts, 10000, other)["passes"])
            for role in colors:
                incomplete = counts.copy()
                incomplete.pop(tuple((colors[role] >> shift) & 255 for shift in (16, 8, 0)))
                self.assertFalse(evaluate_counts(incomplete, 10000, mode)["passes"])

    def test_geometry_oracle_detects_movement_and_identity_loss(self):
        node = {"path": "/heading/1", "role": "heading", "name": "Title", "bounds": {"y": 12}}
        baseline = semantic_geometry({"nodes": [node]})
        for changed in ({**node, "bounds": {"y": 20}}, {**node, "path": "/heading/2"}):
            self.assertNotEqual(baseline, semantic_geometry({"nodes": [changed]}))

    def test_portal_refuses_unowned_bus(self):
        for environment in ({}, {"DBUS_SESSION_BUS_ADDRESS": "unix:path=/desktop"}):
            with self.assertRaises(RuntimeError):
                require_private_bus(environment)

    def test_private_portal_reads_switches_and_releases_name(self):
        with private_bus(os.environ, enabled=False) as environment:
            command = ["gdbus", "call", "--session", "--dest", "org.freedesktop.portal.Desktop",
                       "--object-path", "/org/freedesktop/portal/desktop", "--method",
                       "org.freedesktop.portal.Settings.Read", "org.freedesktop.appearance", "color-scheme"]
            with settings_portal(environment, "dark"):
                before = subprocess.run(command, env=environment, check=True, capture_output=True, text=True, timeout=5)
                self.assertIn("uint32 1", before.stdout)
                set_appearance(environment, "light")
                after = subprocess.run(command, env=environment, check=True, capture_output=True, text=True, timeout=5)
                self.assertIn("uint32 2", after.stdout)
            stopped = subprocess.run(command, env=environment, capture_output=True, text=True, timeout=5)
            self.assertNotEqual(stopped.returncode, 0)


if __name__ == "__main__":
    unittest.main()
