import unittest

from task_toggle_check import toggled_source


class TaskToggleOracleTests(unittest.TestCase):
    def test_only_selected_marker_changes(self):
        source = b"# Tasks\n\n- [x] One\n- [ ] Two\n\n| Key | Value |\n| - | - |\n"
        self.assertEqual(toggled_source(source, "Two"), source.replace(b"[ ] Two", b"[x] Two"))
        self.assertEqual(toggled_source(source, "One"), source.replace(b"[x] One", b"[ ] One"))

    def test_preserves_nested_ordered_uppercase_crlf_and_unicode(self):
        source = "  2) [X] Vérifier\r\n".encode()
        self.assertEqual(toggled_source(source, "Vérifier"), source.replace(b"[X]", b"[ ]"))

    def test_rejects_nonunique_partial_and_non_task_labels(self):
        for source, label in [(b"- [ ] Same\n- [x] Same\n", "Same"),
                              (b"- [ ] Longer label\n", "Longer"),
                              (b"- Plain\n", "Plain")]:
            with self.assertRaises(RuntimeError):
                toggled_source(source, label)


if __name__ == "__main__":
    unittest.main()
