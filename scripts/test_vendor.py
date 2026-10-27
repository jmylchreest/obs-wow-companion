"""Keep the GTK3 compatibility backport confined to its reviewed upstream fix."""
import hashlib
import json
from pathlib import Path
import unittest


class VendorTests(unittest.TestCase):
    def test_glib_matches_upstream_except_pointer_fix(self):
        root = Path(__file__).resolve().parent.parent / "vendor"
        expected = json.loads((root / "glib-upstream.json").read_text(encoding="utf-8"))["files"]
        actual = {}
        for path in (root / "glib").rglob("*"):
            if not path.is_file():
                continue
            name = path.relative_to(root / "glib").as_posix()
            data = path.read_bytes().replace(b"\r\n", b"\n")
            if name == "src/variant_iter.rs":
                self.assertEqual(data.count(b"let mut p: *mut libc::c_char"), 1)
                self.assertEqual(data.count(b"                &mut p,"), 1)
                data = data.replace(b"let mut p: *mut libc::c_char", b"let p: *mut libc::c_char")
                data = data.replace(b"                &mut p,", b"                &p,")
            actual[name] = hashlib.sha256(data).hexdigest()
        self.assertEqual(actual, expected)


if __name__ == "__main__":
    unittest.main()
